use std::collections::HashMap;
use std::sync::Arc;

use lambda_http::IntoResponse;
use tracing::info;

use htsget_http_core::{get_response_for_get_request, Endpoint};
use htsget_search::htsget::HtsGet;

use crate::handlers::handle_response;

/// Get request reads endpoint
pub async fn get<H: HtsGet + Send + Sync + 'static>(
  id_path: String,
  searcher: Arc<H>,
  mut query: HashMap<String, String>,
  endpoint: Endpoint,
) -> impl IntoResponse {
  info!(query = ?query, "GET request with query");
  query.insert("id".to_string(), id_path);
  handle_response(get_response_for_get_request(searcher, query, endpoint).await)
}
