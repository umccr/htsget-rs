use cfg_if::cfg_if;
use futures::StreamExt;
use futures::stream::FuturesOrdered;
use htsget_config::config::advanced::auth::AuthorizationRule;
use htsget_config::types::{JsonResponse, Query, Request, Response};
use htsget_search::HtsGet;
use http::HeaderMap;
use jsonwebtoken::TokenData;
use serde_json::Value;
use tokio::select;
use tracing::debug;
use tracing::instrument;

use crate::HtsGetError::InvalidInput;
use crate::middleware::auth::Auth;
use crate::{
  Endpoint, HtsGetError, PostRequest, Result, convert_to_query, match_format_from_query,
  merge_responses,
};

async fn authenticate(
  headers: &HeaderMap,
  auth: Option<Auth>,
) -> Result<Option<(TokenData<Value>, Auth)>> {
  if let Some(auth) = auth {
    Ok(Some((auth.validate_jwt(headers).await?, auth)))
  } else {
    Ok(None)
  }
}

async fn authorize(
  headers: &HeaderMap,
  path: &str,
  queries: &mut [Query],
  auth: Option<(TokenData<Value>, Auth)>,
) -> Result<Option<Vec<AuthorizationRule>>> {
  if let Some((_, auth)) = auth {
    let _rules = auth.validate_authorization(headers, path, queries).await?;
    cfg_if! {
      if #[cfg(feature = "experimental")] {
        if auth.config().add_hint() {
          Ok(_rules)
        } else {
          Ok(None)
        }
      } else {
        Ok(None)
      }
    }
  } else {
    Ok(None)
  }
}

/// Gets a JSON response for a GET request. The GET request parameters must
/// be in a HashMap. The "id" field is the only mandatory one. The rest can be
/// consulted [here](https://samtools.github.io/hts-specs/htsget.html)
#[instrument(level = "debug", skip_all, ret)]
pub async fn get(
  searcher: impl HtsGet + Send + Sync + 'static,
  request: Request,
  endpoint: Endpoint,
  auth: Option<Auth>,
) -> Result<JsonResponse> {
  let path = request.path().to_string();
  let headers = request.headers().clone();

  let auth = authenticate(&headers, auth).await?;

  let format = match_format_from_query(&endpoint, request.query())?;
  let mut query = vec![convert_to_query(request, format)?];
  let _rules = authorize(&headers, &path, query.as_mut_slice(), auth).await?;

  debug!(endpoint = ?endpoint, query = ?query, "getting GET response");

  let response = searcher
    .search(query.into_iter().next().expect("single element vector"))
    .await
    .map(JsonResponse::from)?;

  cfg_if! {
    if #[cfg(feature = "experimental")] {
      Ok(response.with_allowed(_rules))
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
  auth: Option<Auth>,
) -> Result<JsonResponse> {
  let path = request.path().to_string();
  let headers = request.headers().clone();

  let auth = authenticate(&headers, auth).await?;

  if !request.query().is_empty() {
    return Err(InvalidInput(
      "query parameters should be empty for a POST request".to_string(),
    ));
  }

  let mut queries = body.get_queries(request, &endpoint)?;
  let _rules = authorize(&headers, &path, queries.as_mut_slice(), auth).await?;

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
      Ok(response.with_allowed(_rules))
    } else {
      Ok(response)
    }
  }
}
