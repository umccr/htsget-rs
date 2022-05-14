use std::sync::Arc;

use lambda_http::IntoResponse;
use tracing::info;

use htsget_http_core::{get_response_for_post_request, Endpoint, PostRequest};
use htsget_search::htsget::HtsGet;

use crate::handlers::handle_response;

/// Post request reads endpoint
pub async fn post<H: HtsGet + Send + Sync + 'static>(
  id_path: String,
  searcher: Arc<H>,
  query: PostRequest,
  endpoint: Endpoint,
) -> impl IntoResponse {
  info!(query = ?query, "POST request");
  handle_response(get_response_for_post_request(searcher, query, id_path, endpoint).await)
}
