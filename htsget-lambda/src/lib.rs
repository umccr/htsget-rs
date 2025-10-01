//! htsget-lambda only has functionality to run the Lambda handler. Please use `htsget-axum`
//! for similar functionality on routers and logic.
//!

use futures::TryFuture;
use htsget_axum::server::ticket::TicketServer;
use htsget_config::config::Config;
use htsget_config::package_info;
use lambda_http::request::LambdaRequest;
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

    // After creating the request event, add the original request as an extension.
    event.extensions_mut().insert(original_request);

    let fut = Box::pin(self.service.call(event.with_lambda_context(req.context)));
    TransformResponse::Request(request_origin, fut)
  }
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
