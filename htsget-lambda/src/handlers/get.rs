use std::collections::HashMap;
use std::sync::Arc;

use lambda_http::http;
use tracing::info;
use tracing::instrument;

use htsget_http::{get_response_for_get_request, Endpoint};
use htsget_search::htsget::HtsGet;

use crate::handlers::handle_response;
use crate::{Body, Response};

/// Get request reads endpoint
#[instrument(skip(searcher))]
pub async fn get<H: HtsGet + Send + Sync + 'static>(
  id_path: String,
  searcher: Arc<H>,
  mut query: HashMap<String, String>,
  endpoint: Endpoint,
) -> http::Result<Response<Body>> {
  info!(query = ?query, "GET request");
  query.insert("id".to_string(), id_path);
  handle_response(get_response_for_get_request(searcher, query, endpoint).await)
}
