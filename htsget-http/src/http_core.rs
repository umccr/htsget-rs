use std::collections::HashMap;
use std::sync::Arc;

use futures::stream::FuturesOrdered;
use futures::StreamExt;
use tokio::select;
use tracing::debug;
use tracing::instrument;

use htsget_config::types::{JsonResponse, Request, Response};
use htsget_search::htsget::HtsGet;

use crate::HtsGetError::InvalidInput;
use crate::{
  convert_to_query, match_format, merge_responses, Endpoint, HtsGetError, PostRequest, Result,
};

/// Gets a JSON response for a GET request. The GET request parameters must
/// be in a HashMap. The "id" field is the only mandatory one. The rest can be
/// consulted [here](https://samtools.github.io/hts-specs/htsget.html)
#[instrument(level = "debug", skip_all, ret)]
pub async fn get(
  searcher: Arc<impl HtsGet + Send + Sync + 'static>,
  request: Request,
  endpoint: Endpoint,
) -> Result<JsonResponse> {
  let format = match_format(&endpoint, request.query().get("format"))?;
  let headers = request.headers().clone();

  let query = convert_to_query(request, format)?;

  debug!(endpoint = ?endpoint, query = ?query, "getting GET response");

  if headers.contains_key("authorization") {
    let auth = headers.get("authorization").unwrap().to_str().unwrap();

    let possible_passport = auth.strip_prefix("Bearer ");

    if possible_passport.is_some() {
      let passport = possible_passport.unwrap();

      use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};

      // so obviously these would not be hard coded in the source code in a real deployment
      // but they could very well be hardcoded
      // in the configuration as an expression of "things we trust" (i.e. these values represent the public
      // keys of the entities we trust)
      // the other mechanism would be to trust via URLs - and we would have to fetch these values via
      // OIDC discovery and JWKS
      // e.g. https://broker-australia.aai.dev.umccr.org/jwks
      let e = "AQAB";
      let _issuer_ega_decoding_key = DecodingKey::from_rsa_components("j6F0O-jGn3Ku4hWro21xgrATtrAmOCDsULzMO2pKmUYllUqdnXo-SUE5z1yjzUpuyQ8MAKgP5jgCr5p80tH4Q_Pp_ljEupbVBNh992pUd0EH3DmzCiP-O54NfE2iIlrYi-0hPoQFCHcoWDvFkNtwMA1h3rgN4VR0CgRKjXbkmB7rhDonZvn3eTHIf3_y6cX3Uqk4GJ_sLHAcfjzYOzwfQwH1Mhto3aGhZyv2Vxm9B63IsY7hqkBBxvHfNzM9TWhvm6_0TeDW4Dhiovtjln2aFbVfEa9qJarUR76PnfQc_3eOv4p-gHSP_wyl9gM--8g75ClYR6NJmRBuSaEAz5mpIw", e).unwrap();
      let broker_au_decoding_key = DecodingKey::from_rsa_components("q-mBB5jBJCCc68ALWF_A-zOM4S5gsKPB6qeFYWe_uzkfWf-jcSCHRrRCsMRzflVvZbz3mbBmqp8FVOnEWQe62x0qdNSZRUmRWkeBKhi0yxjXbKV7e11Sv5XWxxGhYL-gYzJXqQLR9T8ZfcDeQvEtobznm51VkZ1UgD6QogjtpCK-LL3t5NK2wS5lZO3K5GM4spbnXOLbbUU0sRHKujkYa6frY71i3EAs_nrzkTRmT0I_QkE24XlGRh0zbM67pW1n9SKHsEpFIEzXy3ebBfHhaWKdxY4qhGqbOvft-rgNGGAXPEkKbygIcE0Uif1DvaNHD6KDAeP0DGJVr3qCKQ4Naw", e).unwrap();

      let header = decode_header(passport).unwrap();

      let validation = {
        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&["https://foo.com"]);
        validation.validate_exp = true;
        validation
      };

      let decoded_token_result = decode::<HashMap<String, serde_json::Value>>(
        passport,
        &broker_au_decoding_key,
        &validation,
      );

      //if decoded_token_result.is_err() {
      //  return JsonResponse::new();
      //}

      // if decoded_token.claims.contains_key("ga4gh_passport_v1") {}

      println!("{:#?}", decoded_token_result.unwrap());
    }
  }

  searcher
    .search(query)
    .await
    .map_err(Into::into)
    .map(JsonResponse::from)
}

/// Gets a response in JSON for a POST request.
/// The parameters can be consulted [here](https://samtools.github.io/hts-specs/htsget.html)
#[instrument(level = "debug", skip_all, ret)]
pub async fn post(
  searcher: Arc<impl HtsGet + Send + Sync + 'static>,
  body: PostRequest,
  request: Request,
  endpoint: Endpoint,
) -> Result<JsonResponse> {
  if !request.query().is_empty() {
    return Err(InvalidInput(
      "query parameters should be empty for a POST request".to_string(),
    ));
  }

  let queries = body.get_queries(request, &endpoint)?;

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

  Ok(JsonResponse::from(
    merge_responses(responses).expect("expected at least one response"),
  ))
}
