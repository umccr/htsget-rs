use std::collections::HashMap;
use std::sync::Arc;

use futures::stream::FuturesOrdered;
use futures::StreamExt;
use tokio::select;
use tracing::debug;

use htsget_search::htsget::Response;
use htsget_search::htsget::{HtsGet, JsonResponse};

use crate::{
  convert_to_query, match_endpoints_get_request, match_endpoints_post_request, merge_responses,
  Endpoint, HtsGetError, PostRequest, Result,
};

/// Gets a JSON response for a GET request. The GET request parameters must
/// be in a HashMap. The "id" field is the only mandatory one. The rest can be
/// consulted [here](https://samtools.github.io/hts-specs/htsget.html)
pub async fn get_response_for_get_request(
  searcher: Arc<impl HtsGet + Send + Sync + 'static>,
  mut query_information: HashMap<String, String>,
  endpoint: Endpoint,
) -> Result<JsonResponse> {
  match_endpoints_get_request(&endpoint, &mut query_information)?;
  debug!(
    ?endpoint,
    ?query_information,
    "Getting GET response for endpoint {:?}, with query {:?}.",
    endpoint,
    query_information
  );

  let query = convert_to_query(&query_information)?;
  let search_result = searcher.search(query).await;

  search_result.map_err(Into::into).map(JsonResponse::from)
}

/// Gets a response in JSON for a POST request.
/// The parameters can be consulted [here](https://samtools.github.io/hts-specs/htsget.html)
pub async fn get_response_for_post_request(
  searcher: Arc<impl HtsGet + Send + Sync + 'static>,
  mut request: PostRequest,
  id: impl Into<String>,
  endpoint: Endpoint,
) -> Result<JsonResponse> {
  match_endpoints_post_request(&endpoint, &mut request)?;
  debug!(
    ?endpoint,
    ?request,
    "Getting POST response for endpoint {:?}, with query {:?}.",
    endpoint,
    request
  );

  let mut futures = FuturesOrdered::new();
  for query in request.get_queries(id)? {
    let owned_searcher = searcher.clone();
    futures.push(tokio::spawn(
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
    // It's okay to unwrap because there will be at least one response
    merge_responses(responses).expect("expected valid response"),
  ))
}
