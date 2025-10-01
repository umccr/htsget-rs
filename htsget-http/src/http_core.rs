use crate::HtsGetError::{InternalError, InvalidInput};
use crate::middleware::auth::Auth;
use crate::{
  Endpoint, HtsGetError, PostRequest, Result, convert_to_query, match_format_from_query,
  merge_responses,
};
use cfg_if::cfg_if;
use futures::StreamExt;
use futures::stream::FuturesOrdered;
use htsget_config::config::advanced::auth::AuthorizationRestrictions;
use htsget_config::config::service_info::PackageInfo;
use htsget_config::types::{JsonResponse, Query, Request, Response};
use htsget_search::HtsGet;
use http::HeaderMap;
use jsonwebtoken::TokenData;
use serde_json::Value;
use tokio::select;
use tracing::debug;
use tracing::instrument;

async fn authenticate(
  headers: &HeaderMap,
  auth: Option<Auth>,
) -> Result<Option<(TokenData<Value>, Auth)>> {
  if let Some(mut auth) = auth {
    if auth.config().auth_mode().is_some() {
      return Ok(Some((auth.validate_jwt(headers).await?, auth)));
    }
  }

  Ok(None)
}

async fn authorize(
  headers: &HeaderMap,
  path: &str,
  queries: &mut [Query],
  auth: Option<(TokenData<Value>, Auth)>,
  extensions: Option<Value>,
) -> Result<Option<AuthorizationRestrictions>> {
  if let Some((_, mut auth)) = auth {
    let _rules = auth
      .validate_authorization(headers, path, queries, extensions)
      .await?;
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
  package_info: Option<&PackageInfo>,
  extensions: Option<Value>,
) -> Result<JsonResponse> {
  let path = request.path().to_string();
  let headers = request.headers().clone();

  let auth = authenticate(&headers, auth).await?;

  let format = match_format_from_query(&endpoint, request.query())?;
  let mut query = vec![convert_to_query(request, format)?];
  let rules = authorize(&headers, &path, query.as_mut_slice(), auth, extensions).await?;

  debug!(endpoint = ?endpoint, query = ?query, "getting GET response");

  let query = query.into_iter().next().expect("single element vector");

  let response = if let Some(ref rules) = rules {
    let mut remote_locations = rules.clone().into_remote_locations();
    if let Some(package_info) = package_info {
      remote_locations
        .set_from_package_info(package_info)
        .map_err(|_| InternalError("invalid remote locations".to_string()))?;
    }

    // If there are remote locations, try them first.
    match remote_locations
      .search(query.clone())
      .await
      .map(JsonResponse::from)
    {
      Ok(response) => response,
      Err(_) => searcher.search(query).await.map(JsonResponse::from)?,
    }
  } else {
    searcher.search(query).await.map(JsonResponse::from)?
  };

  cfg_if! {
    if #[cfg(feature = "experimental")] {
      Ok(response.with_allowed(rules.map(|r| r.into_rules())))
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
  package_info: Option<&PackageInfo>,
  extensions: Option<Value>,
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
  let rules = authorize(&headers, &path, queries.as_mut_slice(), auth, extensions).await?;

  debug!(endpoint = ?endpoint, queries = ?queries, "getting POST response");

  let mut futures = FuturesOrdered::new();
  if let Some(ref rules) = rules {
    for query in queries {
      let mut remote_locations = rules.clone().into_remote_locations();
      if let Some(package_info) = package_info {
        remote_locations
          .set_from_package_info(package_info)
          .map_err(|_| InternalError("invalid remote locations".to_string()))?;
      }
      let owned_searcher = searcher.clone();

      // If there are remote locations, try them first.
      futures.push_back(tokio::spawn(async move {
        match remote_locations.search(query.clone()).await {
          Ok(response) => Ok(response),
          Err(_) => owned_searcher.search(query).await,
        }
      }));
    }
  } else {
    for query in queries {
      let owned_searcher = searcher.clone();
      futures.push_back(tokio::spawn(
        async move { owned_searcher.search(query).await },
      ));
    }
  };

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
      Ok(response.with_allowed(rules.map(|r| r.into_rules())))
    } else {
      Ok(response)
    }
  }
}
