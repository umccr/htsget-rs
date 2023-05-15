use std::collections::HashMap;
use std::sync::Arc;

use lambda_http::http;
use lambda_http::http::HeaderMap;
use tracing::info;
use tracing::instrument;

use htsget_http::{get as htsget_get, Endpoint, Request};
use htsget_search::htsget::HtsGet;

use crate::handlers::handle_response;
use crate::{Body, Response};

/// Get request reads endpoint
#[instrument(skip(searcher))]
pub async fn get<H: HtsGet + Send + Sync + 'static>(
  id: String,
  searcher: Arc<H>,
  query: HashMap<String, String>,
  headers: HeaderMap,
  endpoint: Endpoint,
) -> http::Result<Response<Body>> {
  let request = Request::new(id, query, headers);

  info!(request = ?request, "GET request");

  handle_response(htsget_get(searcher, request, endpoint).await)
}
