use crate::error::Result as HtsGetResult;
use crate::middleware::error::Error::AuthBuilderError;
use crate::middleware::error::Result;
use crate::{convert_to_query, match_format, Endpoint, HtsGetError};
use htsget_config::config::advanced::auth::{AuthConfig, AuthMode, AuthorizationRestrictions};
use htsget_config::types::Request;
use http::uri::PathAndQuery;
use http::Uri;
use jsonpath_rust::JsonPath;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use regex::Regex;
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
  ) -> HtsGetResult<AuthorizationRestrictions> {
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

  pub async fn validate_restrictions(
    restrictions: AuthorizationRestrictions,
    request: Request,
    endpoint: Endpoint,
  ) -> HtsGetResult<()> {
    // Find all rules matching the path.
    let matching_rules = restrictions
      .htsget_auth()
      .iter()
      .filter(|rule| {
        // If this path is a direct match then just return that.
        if rule.path().strip_prefix("/").unwrap_or(rule.path())
          == request.path().strip_prefix("/").unwrap_or(request.path())
        {
          return true;
        }

        // Otherwise, try and parse it as a regex.
        Regex::new(rule.path()).is_ok_and(|regex| regex.is_match(request.path()))
      })
      .collect::<Vec<_>>();

    // If any of the rules allow all reference names (nothing set in the rule) then the user is authorized.
    let allows_all = matching_rules
      .iter()
      .find(|rule| rule.reference_names().is_none());
    if allows_all.is_some() {
      return Ok(());
    }

    let format = match_format(&endpoint, request.query().get("format"))?;
    let query = convert_to_query(request, format)?;
    let matching_restriction = matching_rules
      .iter()
      .flat_map(|rule| rule.reference_names().unwrap_or_default())
      .find(|restriction| {
        query.reference_name() == Some(restriction.name())
          && (restriction.format().is_none() || Some(query.format()) == restriction.format())
          && restriction
            .interval()
            .contains(query.interval().start().unwrap_or(u32::MIN))
          && restriction
            .interval()
            .contains(query.interval().end().unwrap_or(u32::MAX))
      });

    // If the matching rule with the restriction was found, then the user is authorized, otherwise
    // it is a permission denied response.
    if matching_restriction.is_some() {
      Ok(())
    } else {
      Err(HtsGetError::PermissionDenied(
        "failed to authorize user based on authorization service restrictions".to_string(),
      ))
    }
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
