//! Authentication middleware for htsget-axum.
//!

use crate::error::{HtsGetError, HtsGetResult};
use axum::extract::Request;
use axum::response::Response;
use axum::RequestExt;
use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::Authorization;
use axum_extra::TypedHeader;
use futures::future::BoxFuture;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, TokenData, Validation};
use reqwest;
use std::task::{Context, Poll};
use tower::{Layer, Service};

#[derive(Debug, Clone)]
pub struct AuthLayer;

impl<S> Layer<S> for AuthLayer {
  type Service = AuthMiddleware<S>;

  fn layer(&self, inner: S) -> Self::Service {
    AuthMiddleware::new(inner)
  }
}

#[derive(Clone)]
pub struct AuthMiddleware<S> {
  inner: S,
  client: reqwest::Client,
}

impl<S> AuthMiddleware<S> {
  /// New function with default client.
  pub fn new(inner: S) -> Self {
    Self {
      inner,
      client: reqwest::Client::new(),
    }
  }

  /// Fetch JWKS from the authorization server.
  pub async fn fetch_jwks(&self, jwks_url: &str) -> HtsGetResult<JwkSet> {
    let err = || {
      HtsGetError::internal_error("failed to fetch jwks.json from authorization server".to_string())
    };
    let response = self.client.get(jwks_url).send().await.map_err(|_| err())?;

    response.json().await.map_err(|_| err())
  }

  pub async fn validate_authorization(
    &self,
    request: &mut Request,
  ) -> HtsGetResult<TokenData<serde_json::Value>> {
    let auth_token = request
      .extract_parts::<TypedHeader<Authorization<Bearer>>>()
      .await
      .map_err(|err| HtsGetError::permission_denied(err.to_string()))?;

    // Placeholder authorization server.
    let jwks_url = "/.well-known/jwks.json".to_string();

    // Decode header and get the key id.
    let header = decode_header(auth_token.token())?;
    let kid = header
      .kid
      .ok_or_else(|| HtsGetError::permission_denied("JWT missing key ID".to_string()))?;

    // Fetch JWKS from the authorization server and find matching JWK.
    let jwks = self.fetch_jwks(&jwks_url).await?;
    let matched_jwk = jwks
      .find(&kid)
      .ok_or_else(|| HtsGetError::permission_denied("matching JWK not found".to_string()))?;

    // Decode and validate the JWT
    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_exp = true;

    match decode(
      auth_token.token(),
      &DecodingKey::from_jwk(matched_jwk)?,
      &validation,
    ) {
      Ok(claims) => Ok(claims),
      Err(err) => Err(HtsGetError::permission_denied(format!(
        "invalid JWT: {err}"
      ))),
    }
  }
}

impl<S> Service<Request> for AuthMiddleware<S>
where
  S: Service<Request, Response = Response> + Clone + Send + 'static,
  S::Future: Send + 'static,
{
  type Response = S::Response;
  type Error = S::Error;
  type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

  fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
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
