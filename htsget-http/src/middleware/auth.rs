use crate::error::Result as HtsGetResult;
use crate::middleware::error::Error::AuthBuilderError;
use crate::middleware::error::Result;
use crate::HtsGetError;
use htsget_config::config::advanced::auth::{AuthConfig, AuthMode, AuthorizationResponse};
use http::uri::PathAndQuery;
use http::Uri;
use jsonpath_rust::JsonPath;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::de::DeserializeOwned;
use serde_json::Value;

#[derive(Default, Debug)]
pub struct AuthBuilder {
  config: Option<AuthConfig>,
}

impl AuthBuilder {
  /// Set the config.
  pub fn with_config(mut self, config: AuthConfig) -> Self {
    self.config = Some(config);
    self
  }

  pub fn build(self) -> Result<Auth> {
    let Some(mut config) = self.config else {
      return Err(AuthBuilderError("missing config".to_string()));
    };

    if config.trusted_authorization_urls().is_empty() {
      return Err(AuthBuilderError(
        "at least one trusted authorization url must be set".to_string(),
      ));
    }
    if config.authorization_path().is_none() && config.trusted_authorization_urls().len() > 1 {
      return Err(AuthBuilderError(
        "only one trusted authorization url should be set when not using authorization paths"
          .to_string(),
      ));
    }

    let mut decoding_key = None;
    match config.auth_mode_mut() {
      AuthMode::Jwks(uri) => {
        let mut jwks_url = uri.clone().into_parts();
        jwks_url.path_and_query = Some(PathAndQuery::from_static("/.well-known/jwks.json"));
        *uri = Uri::from_parts(jwks_url).map_err(|err| AuthBuilderError(err.to_string()))?;
      }
      AuthMode::PublicKey(public_key) => {
        decoding_key = Some(
          Auth::decode_public_key(public_key)
            .map_err(|_| AuthBuilderError("failed to decode public key".to_string()))?,
        );
      }
    }

    Ok(Auth {
      config,
      decoding_key,
    })
  }
}

#[derive(Clone)]
pub struct Auth {
  config: AuthConfig,
  decoding_key: Option<DecodingKey>,
}

impl Auth {
  /// Fetch JWKS from the authorization server.
  pub async fn fetch_from_url<D: DeserializeOwned>(&self, url: &str) -> HtsGetResult<D> {
    let err = || {
      HtsGetError::InternalError("failed to fetch jwks.json from authorization server".to_string())
    };
    let response = self
      .config
      .http_client()
      .get(url)
      .send()
      .await
      .map_err(|_| err())?;

    response.json().await.map_err(|_| err())
  }

  /// Get a decoding key form the JWKS url.
  pub async fn decode_jwks(&self, jwks_url: &Uri, token: &str) -> HtsGetResult<DecodingKey> {
    // Decode header and get the key id.
    let header = decode_header(token)?;
    let kid = header
      .kid
      .ok_or_else(|| HtsGetError::PermissionDenied("JWT missing key ID".to_string()))?;

    // Fetch JWKS from the authorization server and find matching JWK.
    let jwks = self.fetch_from_url::<JwkSet>(&jwks_url.to_string()).await?;
    let matched_jwk = jwks
      .find(&kid)
      .ok_or_else(|| HtsGetError::PermissionDenied("matching JWK not found".to_string()))?;

    Ok(DecodingKey::from_jwk(matched_jwk)?)
  }

  /// Decode a public key into an RSA, EdDSA or ECDSA pem-formatted decoding key.
  pub fn decode_public_key(key: &[u8]) -> HtsGetResult<DecodingKey> {
    Ok(
      DecodingKey::from_rsa_pem(key)
        .or_else(|_| DecodingKey::from_ed_pem(key))
        .or_else(|_| DecodingKey::from_ec_pem(key))?,
    )
  }

  pub async fn query_authorization_service(
    &self,
    claims: Value,
  ) -> HtsGetResult<AuthorizationResponse> {
    let query_url = match self.config.authorization_path() {
      None => self
        .config
        .trusted_authorization_urls()
        .first()
        .ok_or_else(|| {
          HtsGetError::InternalError("missing trusted authorization url".to_string())
        })?,
      Some(path) => {
        // Extract the url from the path.
        let path = claims.query(path).map_err(|err| {
          HtsGetError::InvalidAuthentication(format!(
            "failed to find authorization service in claims: {err}",
          ))
        })?;
        let url = path
          .first()
          .ok_or_else(|| {
            HtsGetError::InvalidAuthentication(
              "expected one value for authorization service in claims".to_string(),
            )
          })?
          .as_str()
          .ok_or_else(|| {
            HtsGetError::InvalidAuthentication(
              "expected string value for authorization service in claims".to_string(),
            )
          })?;
        &url.parse::<Uri>().map_err(|err| {
          HtsGetError::InvalidAuthentication(format!("failed to parse authorization url: {err}"))
        })?
      }
    };

    // Ensure that the authorization url is trusted.
    if !self.config.trusted_authorization_urls().contains(query_url) {
      return Err(HtsGetError::PermissionDenied(
        "authorization service in claims not a trusted authorization url".to_string(),
      ));
    };

    self.fetch_from_url(&query_url.to_string()).await
  }

  pub async fn validate_authorization(&self, token: &str) -> HtsGetResult<()> {
    let decoding_key = if let Some(ref decoding_key) = self.decoding_key {
      decoding_key
    } else {
      match self.config.auth_mode() {
        AuthMode::Jwks(jwks) => &self.decode_jwks(jwks, token).await?,
        AuthMode::PublicKey(public_key) => &Self::decode_public_key(public_key)?,
      }
    };

    // Decode and validate the JWT
    let mut validation = Validation::default();
    validation.algorithms = vec![Algorithm::RS256, Algorithm::ES256];
    validation.validate_exp = true;
    validation.validate_aud = true;
    validation.validate_nbf = true;

    if let Some(iss) = self.config.validate_issuer() {
      validation.set_issuer(iss);
      validation.required_spec_claims.insert("iss".to_string());
    }
    if let Some(aud) = self.config.validate_audience() {
      validation.set_audience(aud);
      validation.required_spec_claims.insert("aud".to_string());
    }
    if let Some(sub) = self.config.validate_subject() {
      validation.sub = Some(sub.to_string());
      validation.required_spec_claims.insert("sub".to_string());
    }

    let claims = match decode::<Value>(token, decoding_key, &validation) {
      Ok(claims) => claims,
      Err(err) => return Err(HtsGetError::PermissionDenied(format!("invalid JWT: {err}"))),
    };

    self.query_authorization_service(claims.claims).await?;

    Ok(())
  }
}
