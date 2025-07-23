//! Authentication middleware for htsget-axum.
//!

use crate::error::HtsGetError;
use axum::extract::Request;
use axum::response::{IntoResponse, Response};
use axum::RequestExt;
use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::Authorization;
use axum_extra::TypedHeader;
use futures::future::BoxFuture;
use http::header::AUTHORIZATION;
use std::task::{Context, Poll};
use tower::{Layer, Service};

#[derive(Debug, Clone)]
pub struct AuthLayer;

impl<S> Layer<S> for AuthLayer {
  type Service = AuthMiddleware<S>;

  fn layer(&self, inner: S) -> Self::Service {
    AuthMiddleware { inner }
  }
}

#[derive(Clone)]
pub struct AuthMiddleware<S> {
  inner: S,
}

impl<S> AuthMiddleware<S>
where
  S: Service<Request, Response = Response> + Send + 'static,
  S::Future: Send + 'static,
{
  fn pin_future(response: Response) -> BoxFuture<'static, Result<S::Response, S::Error>> {
    Box::pin(async move { Ok(response) })
  }
}

impl<S> AuthMiddleware<S> {
  pub async fn validate_authorization(request: &mut Request) -> Result<(), impl IntoResponse> {
    let auth_token = request
      .extract_parts::<TypedHeader<Authorization<Bearer>>>()
      .await
      .map_err(|err| HtsGetError::permission_denied(err.to_string()).into_response())?
      .token();

    // let mut validation = Validation::new(Algorithm::HS256);

    Ok::<_, Response>(())
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
