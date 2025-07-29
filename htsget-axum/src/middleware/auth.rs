//! Authentication middleware for htsget-axum.
//!

use crate::error::Error::AuthBuilderError;
use crate::error::Result;
use crate::error::{HtsGetError, HtsGetResult};
use axum::extract::Request;
use axum::response::Response;
use axum::RequestExt;
use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::Authorization;
use axum_extra::TypedHeader;
use futures::future::BoxFuture;
use htsget_config::config::advanced::auth::{AuthConfig, AuthMode, AuthorizationResponse};
use http::uri::PathAndQuery;
use http::Uri;
use jsonpath_rust::JsonPath;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::result;
use std::task::{Context, Poll};
use tower::{Layer, Service};

#[derive(Default, Debug)]
pub struct AuthLayerBuilder {
  config: Option<AuthConfig>,
}

impl AuthLayerBuilder {
  /// Set the config.
  pub fn with_config(mut self, config: AuthConfig) -> Self {
    self.config = Some(config);
    self
  }

  pub fn build(self) -> Result<AuthLayer> {
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
          AuthMiddleware::<Self>::decode_public_key(public_key)
            .map_err(|_| AuthBuilderError("failed to decode public key".to_string()))?,
        );
      }
    }

    Ok(AuthLayer {
      config,
      decoding_key,
    })
  }
}

#[derive(Clone)]
pub struct AuthLayer {
  config: AuthConfig,
  decoding_key: Option<DecodingKey>,
}

impl<S> Layer<S> for AuthLayer {
  type Service = AuthMiddleware<S>;

  fn layer(&self, inner: S) -> Self::Service {
    AuthMiddleware::new(inner, self.clone(), self.decoding_key.clone())
  }
}

#[derive(Clone)]
pub struct AuthMiddleware<S> {
  inner: S,
  layer: AuthLayer,
  decoding_key: Option<DecodingKey>,
}

impl<S> AuthMiddleware<S> {
  /// New function with default client.
  pub fn new(inner: S, layer: AuthLayer, decoding_key: Option<DecodingKey>) -> Self {
    Self {
      inner,
      layer,
      decoding_key,
    }
  }

  /// Fetch JWKS from the authorization server.
  pub async fn fetch_from_url<D: DeserializeOwned>(&self, url: &str) -> HtsGetResult<D> {
    let err = || {
      HtsGetError::internal_error("failed to fetch jwks.json from authorization server".to_string())
    };
    let response = self
      .layer
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
      .ok_or_else(|| HtsGetError::permission_denied("JWT missing key ID".to_string()))?;

    // Fetch JWKS from the authorization server and find matching JWK.
    let jwks = self.fetch_from_url::<JwkSet>(&jwks_url.to_string()).await?;
    let matched_jwk = jwks
      .find(&kid)
      .ok_or_else(|| HtsGetError::permission_denied("matching JWK not found".to_string()))?;

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
    let query_url = match self.layer.config.authorization_path() {
      None => self
        .layer
        .config
        .trusted_authorization_urls()
        .first()
        .ok_or_else(|| {
          HtsGetError::internal_error("missing trusted authorization url".to_string())
        })?,
      Some(path) => {
        // Extract the url from the path.
        let path = claims.query(path).map_err(|err| {
          HtsGetError::permission_denied(format!(
            "failed to find authorization service in claims: {err}",
          ))
        })?;
        let url = path
          .first()
          .ok_or_else(|| {
            HtsGetError::permission_denied(
              "expected one value for authorization service in claims".to_string(),
            )
          })?
          .as_str()
          .ok_or_else(|| {
            HtsGetError::permission_denied(
              "expected string value for authorization service in claims".to_string(),
            )
          })?;
        &url.parse::<Uri>().map_err(|err| {
          HtsGetError::permission_denied(format!("failed to parse authorization url: {err}"))
        })?
      }
    };

    // Ensure that the authorization url is trusted.
    if !self
      .layer
      .config
      .trusted_authorization_urls()
      .contains(query_url)
    {
      return Err(HtsGetError::permission_denied(
        "authorization service in claims not a trusted authorization url".to_string(),
      ));
    };

    self.fetch_from_url(&query_url.to_string()).await
  }

  pub async fn validate_authorization(&self, request: &mut Request) -> HtsGetResult<()> {
    let auth_token = request
      .extract_parts::<TypedHeader<Authorization<Bearer>>>()
      .await
      .map_err(|err| HtsGetError::permission_denied(err.to_string()))?;

    let decoding_key = if let Some(ref decoding_key) = self.decoding_key {
      decoding_key
    } else {
      match self.layer.config.auth_mode() {
        AuthMode::Jwks(jwks) => &self.decode_jwks(jwks, auth_token.token()).await?,
        AuthMode::PublicKey(public_key) => &Self::decode_public_key(public_key)?,
      }
    };

    // Decode and validate the JWT
    let mut validation = Validation::default();
    validation.algorithms = vec![Algorithm::RS256, Algorithm::ES256];
    validation.validate_exp = true;
    validation.validate_aud = true;
    validation.validate_nbf = true;

    if let Some(iss) = self.layer.config.validate_issuer() {
      validation.set_issuer(iss);
      validation.required_spec_claims.insert("iss".to_string());
    }
    if let Some(aud) = self.layer.config.validate_audience() {
      validation.set_audience(aud);
      validation.required_spec_claims.insert("aud".to_string());
    }
    if let Some(sub) = self.layer.config.validate_subject() {
      validation.sub = Some(sub.to_string());
      validation.required_spec_claims.insert("sub".to_string());
    }

    let claims = match decode::<Value>(auth_token.token(), decoding_key, &validation) {
      Ok(claims) => claims,
      Err(err) => {
        return Err(HtsGetError::permission_denied(format!(
          "invalid JWT: {err}"
        )))
      }
    };

    self.query_authorization_service(claims.claims).await?;

    Ok(())
  }
}

impl<S> Service<Request> for AuthMiddleware<S>
where
  S: Service<Request, Response = Response> + Clone + Send + 'static,
  S::Future: Send + 'static,
{
  type Response = S::Response;
  type Error = S::Error;
  type Future = BoxFuture<'static, result::Result<Self::Response, Self::Error>>;

  fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<result::Result<(), Self::Error>> {
    self.inner.poll_ready(cx)
  }

  fn call(&mut self, request: Request) -> Self::Future {
    let clone = self.inner.clone();
    // The inner service must be ready so we replace it with the cloned value.
    // See https://docs.rs/tower/latest/tower/trait.Service.html#be-careful-when-cloning-inner-services
    let mut inner = std::mem::replace(&mut self.inner, clone);
    Box::pin(async move {
      let response: Response = inner.call(request).await?;
      Ok(response)
    })
  }
}
