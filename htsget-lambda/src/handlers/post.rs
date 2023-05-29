use std::collections::HashMap;
use std::sync::Arc;

use lambda_http::http;
use lambda_http::http::HeaderMap;
use tracing::info;
use tracing::instrument;

use htsget_config::types::Request;
use htsget_http::{post as htsget_post, Endpoint, PostRequest};
use htsget_search::htsget::HtsGet;

use crate::handlers::handle_response;
use crate::{Body, Response};

/// Post request reads endpoint
#[instrument(skip(searcher))]
pub async fn post<H: HtsGet + Send + Sync + 'static>(
  id: String,
  searcher: Arc<H>,
  query: HashMap<String, String>,
  body: PostRequest,
  headers: HeaderMap,
  endpoint: Endpoint,
) -> http::Result<Response<Body>> {
  let request = Request::new(id, query, headers);

  info!(body = ?body, "POST request");

  handle_response(htsget_post(searcher, body, request, endpoint).await)
}
