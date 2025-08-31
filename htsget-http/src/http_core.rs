use cfg_if::cfg_if;
use futures::StreamExt;
use futures::stream::FuturesOrdered;
use tokio::select;
use tracing::debug;
use tracing::instrument;

use htsget_config::types::{JsonResponse, Request, Response, SuppressedRequest};
use htsget_search::HtsGet;

use crate::HtsGetError::InvalidInput;
use crate::{
  Endpoint, HtsGetError, PostRequest, Result, convert_to_query, match_format_from_query,
  merge_responses,
};

/// Gets a JSON response for a GET request. The GET request parameters must
/// be in a HashMap. The "id" field is the only mandatory one. The rest can be
/// consulted [here](https://samtools.github.io/hts-specs/htsget.html)
#[instrument(level = "debug", skip_all, ret)]
pub async fn get(
  searcher: impl HtsGet + Send + Sync + 'static,
  request: Request,
  endpoint: Endpoint,
  mut suppressed_request: Option<SuppressedRequest>,
) -> Result<JsonResponse> {
  let format = match_format_from_query(&endpoint, request.query())?;
  let rules = suppressed_request.as_mut().and_then(|req| {
    if req.add_hint() {
      Some(req.take_matching_rules())
    } else {
      None
    }
  });
  let query = convert_to_query(request, format, suppressed_request)?;

  debug!(endpoint = ?endpoint, query = ?query, "getting GET response");

  let response = searcher.search(query).await.map(JsonResponse::from)?;

  cfg_if! {
    if #[cfg(feature = "experimental")] {
      Ok(response.with_allowed(rules))
    } else {
      Ok(response)
    }
  }
}

/// Gets a response in JSON for a POST request.
/// The parameters can be consulted [here](https://samtools.github.io/hts-specs/htsget.html)
#[instrument(level = "debug", skip_all, ret)]
pub async fn post(
  searcher: impl HtsGet + Clone + Send + Sync + 'static,
  body: PostRequest,
  request: Request,
  endpoint: Endpoint,
  mut suppressed_request: Option<SuppressedRequest>,
) -> Result<JsonResponse> {
  if !request.query().is_empty() {
    return Err(InvalidInput(
      "query parameters should be empty for a POST request".to_string(),
    ));
  }

  let rules = suppressed_request.as_mut().and_then(|req| {
    if req.add_hint() {
      Some(req.take_matching_rules())
    } else {
      None
    }
  });
  let queries = body.get_queries(request, &endpoint, suppressed_request)?;

  debug!(endpoint = ?endpoint, queries = ?queries, "getting POST response");

  let mut futures = FuturesOrdered::new();
  for query in queries {
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

  let response =
    JsonResponse::from(merge_responses(responses).expect("expected at least one response"));
  cfg_if! {
    if #[cfg(feature = "experimental")] {
      Ok(response.with_allowed(rules))
    } else {
      Ok(response)
    }
  }
}
