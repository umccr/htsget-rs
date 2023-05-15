use std::sync::Arc;

use futures::stream::FuturesOrdered;
use futures::StreamExt;
use tokio::select;
use tracing::debug;
use tracing::instrument;

use htsget_config::types::{JsonResponse, Response};
use htsget_search::htsget::HtsGet;

use crate::{
  convert_to_query, match_endpoints_get_request, match_endpoints_post_request, merge_responses,
  Endpoint, HtsGetError, PostRequest, Request, Result,
};

/// Gets a JSON response for a GET request. The GET request parameters must
/// be in a HashMap. The "id" field is the only mandatory one. The rest can be
/// consulted [here](https://samtools.github.io/hts-specs/htsget.html)
#[instrument(level = "debug", skip_all, ret)]
pub async fn get(
  searcher: Arc<impl HtsGet + Send + Sync + 'static>,
  mut request: Request,
  endpoint: Endpoint,
) -> Result<JsonResponse> {
  let query_information = request.query_information_mut();

  match_endpoints_get_request(&endpoint, query_information)?;
  debug!(endpoint = ?endpoint, query_information = ?query_information, "getting GET response");

  let query = convert_to_query(query_information)?;
  let search_result = searcher.search(query).await;

  search_result.map_err(Into::into).map(JsonResponse::from)
}

/// Gets a response in JSON for a POST request.
/// The parameters can be consulted [here](https://samtools.github.io/hts-specs/htsget.html)
#[instrument(level = "debug", skip_all, ret)]
pub async fn post(
  searcher: Arc<impl HtsGet + Send + Sync + 'static>,
  mut request: PostRequest,
  id: impl Into<String>,
  endpoint: Endpoint,
) -> Result<JsonResponse> {
  match_endpoints_post_request(&endpoint, &mut request)?;
  debug!(endpoint = ?endpoint, request = ?request, "getting POST response");

  let mut futures = FuturesOrdered::new();
  for query in request.get_queries(id)? {
    let owned_searcher = searcher.clone();
    futures.push_back(tokio::spawn(
      async move { owned_searcher.search(query).await },
    ));
  }
  let mut responses: Vec<Response> = Vec::new();
  loop {
    select! {
      Some(next) = futures.next() => responses.push(next.map_err(|err| HtsGetError::InternalError(err.to_string()))?.map_err(HtsGetError::from)?),
      else => break
    }
  }

  Ok(JsonResponse::from(
    merge_responses(responses).expect("expected at least one response"),
  ))
}
