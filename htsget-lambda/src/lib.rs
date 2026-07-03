//! htsget-lambda only has functionality to run the Lambda handler. Please use `htsget-axum`
//! for similar functionality on routers and logic.
//!

use futures::TryFuture;
use htsget_axum::server::ticket::TicketServer;
use htsget_config::config::Config;
use htsget_config::package_info;
use lambda_http::http::Uri;
use lambda_http::request::{LambdaRequest, RequestContext};
use lambda_http::{
  Error, IntoResponse, LambdaEvent, Request, RequestExt, Service, TransformResponse, lambda_runtime,
};
use std::marker::PhantomData;
use std::path::Path;
use std::task::{Context, Poll};
use tracing::debug;

/// Wraps the htsget-axum router to forward any Lambda event extensions to the router.
pub struct Adapter<'a, R, S> {
  service: S,
  _phantom_data: PhantomData<&'a R>,
}

impl<'a, R, S, E> From<S> for Adapter<'a, R, S>
where
  S: Service<Request, Response = R, Error = E>,
  S::Future: Send + 'a,
  R: IntoResponse,
{
  fn from(service: S) -> Self {
    Adapter {
      service,
      _phantom_data: PhantomData,
    }
  }
}

impl<'a, R, S, E> Service<LambdaEvent<serde_json::Value>> for Adapter<'a, R, S>
where
  S: Service<Request, Response = R, Error = E>,
  S::Future: Send + 'a,
  R: IntoResponse,
{
  type Response = <TransformResponse<'a, R, E> as TryFuture>::Ok;
  type Error = E;
  type Future = TransformResponse<'a, R, Self::Error>;

  fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    self.service.poll_ready(cx)
  }

  fn call(&mut self, req: LambdaEvent<serde_json::Value>) -> Self::Future {
    // The original request is consumed when creating a future, so clone it here.
    let original_request = req.payload.clone();

    let lambda_request: LambdaRequest =
      serde_json::from_value(req.payload).expect("invalid payload when deserializing to json");
    let request_origin = lambda_request.request_origin();
    let mut event: Request = lambda_request.into();

    strip_stage_from_path(&mut event);

    // After creating the request event, add the original request as an extension.
    debug!(original_request = ?original_request, "original_request");
    event.extensions_mut().insert(original_request);

    let fut = Box::pin(self.service.call(event.with_lambda_context(req.context)));
    TransformResponse::Request(request_origin, fut)
  }
}

/// Strip the API Gateway stage from the start of the request path so that routing works correctly.
fn strip_stage_from_path(event: &mut Request) {
  let stage = match event.request_context_ref() {
    Some(RequestContext::ApiGatewayV1(context)) => context.stage.as_deref(),
    Some(RequestContext::ApiGatewayV2(context)) => context.stage.as_deref(),
    _ => None,
  };

  let Some(stage) = stage.filter(|stage| *stage != "$default") else {
    return;
  };

  let stripped = match event.uri().path().strip_prefix(&format!("/{stage}")) {
    Some("") => "/",
    Some(rest) if rest.starts_with('/') => rest,
    _ => return,
  };

  let path_and_query = match event.uri().query() {
    Some(query) => format!("{stripped}?{query}"),
    None => stripped.to_string(),
  };

  let mut parts = event.uri().clone().into_parts();
  let path_and_query = path_and_query
    .parse()
    .expect("expected valid path and query from a valid uri");
  parts.path_and_query = Some(path_and_query);
  *event.uri_mut() = Uri::from_parts(parts).expect("expected valid parts from a valid uri");
}

/// Run the Lambda handler using the config file contained at the path.
pub async fn run_handler(path: &Path) -> Result<(), Error> {
  let mut config = Config::from_path(path)?;
  config.set_package_info(package_info!())?;
  config.setup_tracing()?;

  debug!(config = ?config, "config parsed");

  let service_info = config.service_info().clone();
  let cors = config.ticket_server().cors().clone();
  let auth = config.ticket_server().auth().cloned();
  let package_info = config.package_info().clone();
  let router = TicketServer::router(
    config.into_locations(),
    service_info,
    cors,
    auth,
    Some(package_info),
  )?;

  lambda_runtime::run(Adapter::from(router)).await
}

#[cfg(test)]
mod tests {
  use super::*;
  use aws_lambda_events::apigw::{ApiGatewayProxyRequestContext, ApiGatewayV2httpRequestContext};
  use lambda_http::Body;

  fn v1_request(uri: &str, stage: Option<&str>) -> Request {
    let mut context = ApiGatewayProxyRequestContext::default();
    context.stage = stage.map(|stage| stage.to_string());
    request(uri).with_request_context(RequestContext::ApiGatewayV1(context))
  }

  fn v2_request(uri: &str, stage: Option<&str>) -> Request {
    let mut context = ApiGatewayV2httpRequestContext::default();
    context.stage = stage.map(|stage| stage.to_string());
    request(uri).with_request_context(RequestContext::ApiGatewayV2(context))
  }

  fn request(uri: &str) -> Request {
    lambda_http::http::Request::builder()
      .uri(uri)
      .body(Body::Empty)
      .unwrap()
  }

  fn strip(mut event: Request) -> String {
    strip_stage_from_path(&mut event);
    event.uri().to_string()
  }

  #[test]
  fn strip_stage() {
    assert_eq!(
      strip(v1_request("/prod/reads/id", Some("prod"))),
      "/reads/id"
    );
    assert_eq!(
      strip(v2_request("/prod/reads/id", Some("prod"))),
      "/reads/id"
    );

    assert_eq!(
      strip(v1_request("/prod/reads/id?format=BAM", Some("prod"))),
      "/reads/id?format=BAM"
    );
    assert_eq!(strip(v1_request("/prod", Some("prod"))), "/");
    assert_eq!(strip(request("/prod/reads/id")), "/prod/reads/id");
    assert_eq!(
      strip(v1_request("/reads/id", Some("$default"))),
      "/reads/id"
    );
    assert_eq!(strip(v1_request("/reads/id", None)), "/reads/id");
    assert_eq!(
      strip(v1_request("/production/reads/id", Some("prod"))),
      "/production/reads/id"
    );
    assert_eq!(strip(v1_request("/reads/id", Some("prod"))), "/reads/id");
  }
}
