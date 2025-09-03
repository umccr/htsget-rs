//! Authentication middleware for htsget-axum.
//!

use crate::error::HtsGetError;
use axum::extract::Request;
use axum::response::{IntoResponse, Response};
use futures::future::BoxFuture;
use htsget_http::middleware::auth::Auth;
use std::task::{Context, Poll};
use tower::{Layer, Service};

impl From<Auth> for AuthenticationLayer {
  fn from(auth: Auth) -> Self {
    Self { inner: auth }
  }
}

/// A wrapper around the authentication layer.
#[derive(Clone)]
pub struct AuthenticationLayer {
  inner: Auth,
}

impl AuthenticationLayer {
  /// Get the inner auth layer.
  pub fn inner(&self) -> &Auth {
    &self.inner
  }
}

impl<S> Layer<S> for AuthenticationLayer {
  type Service = AuthenticationMiddleware<S>;

  fn layer(&self, inner: S) -> Self::Service {
    AuthenticationMiddleware::new(inner, self.clone())
  }
}

/// A wrapper around the authentication middleware. This middleware only handles
/// authentication of a JWT token.
#[derive(Clone)]
pub struct AuthenticationMiddleware<S> {
  inner: S,
  layer: AuthenticationLayer,
}

impl<S> AuthenticationMiddleware<S> {
  /// Create a new middleware auth.
  pub fn new(inner: S, layer: AuthenticationLayer) -> Self {
    Self { inner, layer }
  }

  /// Get the inner service.
  pub fn inner(&self) -> &S {
    &self.inner
  }

  /// Get the layer.
  pub fn layer(&self) -> &AuthenticationLayer {
    &self.layer
  }
}

impl<S> Service<Request> for AuthenticationMiddleware<S>
where
  S: Service<Request, Response = Response> + Clone + Send + 'static + Sync,
  S::Future: Send + 'static,
{
  type Response = S::Response;
  type Error = S::Error;
  type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

  fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    self.inner.poll_ready(cx)
  }

  fn call(&mut self, request: Request) -> Self::Future {
    let clone = self.clone();
    // The inner service must be ready so we replace it with the cloned value.
    // See https://docs.rs/tower/latest/tower/trait.Service.html#be-careful-when-cloning-inner-services
    let mut self_owned = std::mem::replace(self, clone);
    Box::pin(async move {
      if let Err(err) = self_owned.layer.inner.validate_jwt(request.headers()).await {
        return Ok(HtsGetError::from(err).into_response());
      }

      self_owned.inner.call(request).await
    })
  }
}
