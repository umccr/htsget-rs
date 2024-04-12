use std::collections::HashMap;
use std::sync::Arc;

use futures::stream::FuturesOrdered;
use futures::StreamExt;
use jsonwebtoken::Algorithm::RS256;
use regex::Regex;
use tokio::select;
use tracing::instrument;
use tracing::{debug, info};

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
  // this is a hack for our demonstration - the presence of matches to this
  // regex will turn on the need for GA4GH passport auth
  let ega_regex = Regex::new(r".*(EGAD[0-9]{11}).*").unwrap();

  // make a clone of the path as we send the request data off elsewhere
  let ega_request_path = request.path().to_string();

  // return an array of the capture groups that match
  let ega_path: Vec<&str> = ega_regex
    .captures_iter(ega_request_path.as_str())
    .map(|caps| {
      let (_, [ega_dataset]) = caps.extract();
      ega_dataset
    })
    .collect();

  // no capture groups means that EGA was not mentioned in the path and we will not do GA4GH for this demo
  let is_ega = !ega_path.is_empty();

  info!(is_ega = ?is_ega, ega_path = ?ega_path, "should we enable GA4GH passport auth");

  let format = match_format(&endpoint, request.query().get("format"))?;
  let headers = request.headers().clone();

  let query = convert_to_query(request, format)?;

  debug!(endpoint = ?endpoint, query = ?query, "getting GET response");

  // an implementation of decoding/validating authorisation login for GA4GH passports
  if is_ega {
    if !headers.contains_key("authorization") {
      return Err(HtsGetError::InvalidAuthentication(
        "needs Passport bearer token in the Authorization header".to_string(),
      ));
    }

    let auth = headers.get("authorization").unwrap().to_str().unwrap();

    let possible_passport = auth.strip_prefix("Bearer ");

    if possible_passport.is_none() {
      return Err(HtsGetError::InvalidAuthentication(
        "needs Passport bearer token as 'Bearer <token>'".to_string(),
      ));
    }

    let passport = possible_passport.unwrap();

    use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};

    // so obviously these would *not* be hard coded in the source code in a real deployment
    // though they could very well be hardcoded
    // in the configuration as an expression of "things we trust" (i.e. these values represent the public
    // keys of the entities we trust)
    // the other mechanism would be to trust via URLs - and we would have to fetch the public keys via
    // OIDC discovery and JWKS
    // e.g. load and cache https://broker-australia.aai.dev.umccr.org/jwks
    let e = "AQAB";

    // we store a map of issuer -> public RSA "n"
    let brokers = HashMap::from([
        ("https://broker-europe.aai.dev.umccr.org", DecodingKey::from_rsa_components("n5POaIPBp04XYkujy7ILkeYpuqPtzRz6fWNFZy7fR6qPLycP4aANFA2xRjr5YP1XXRnm7Jg23gmSbGYFNlnKDRNf67PM53L9Afx56DAUufH0vAISOq2e-i2P4aWZCGcc-d-8tmNTQ3FFcS2wD3bwUsVG2uLXVcdHvmvbTVVIXYxNiznXLk3sNBjuL40VIKEK_x8KSX04_0_x07KKFW1rqj1sguzBeF-NJRTGKuplFEwVM5TxAXRNQe1VeC3_TAEK4PRD8bzzFBz3y-fyovlppfjeOEbIlLT4mafzD130dlINw4xdaLQIPkQb8UE8O-XNKUzguSdUOw0TYB49mFIm5w", e).unwrap()),
        ("https://broker-australia.aai.dev.umccr.org", DecodingKey::from_rsa_components("q-mBB5jBJCCc68ALWF_A-zOM4S5gsKPB6qeFYWe_uzkfWf-jcSCHRrRCsMRzflVvZbz3mbBmqp8FVOnEWQe62x0qdNSZRUmRWkeBKhi0yxjXbKV7e11Sv5XWxxGhYL-gYzJXqQLR9T8ZfcDeQvEtobznm51VkZ1UgD6QogjtpCK-LL3t5NK2wS5lZO3K5GM4spbnXOLbbUU0sRHKujkYa6frY71i3EAs_nrzkTRmT0I_QkE24XlGRh0zbM67pW1n9SKHsEpFIEzXy3ebBfHhaWKdxY4qhGqbOvft-rgNGGAXPEkKbygIcE0Uif1DvaNHD6KDAeP0DGJVr3qCKQ4Naw", e).unwrap()),
      ]);

    let issuers = HashMap::from([
      ("https://issuer-ega.aai.dev.umccr.org", DecodingKey::from_rsa_components("j6F0O-jGn3Ku4hWro21xgrATtrAmOCDsULzMO2pKmUYllUqdnXo-SUE5z1yjzUpuyQ8MAKgP5jgCr5p80tH4Q_Pp_ljEupbVBNh992pUd0EH3DmzCiP-O54NfE2iIlrYi-0hPoQFCHcoWDvFkNtwMA1h3rgN4VR0CgRKjXbkmB7rhDonZvn3eTHIf3_y6cX3Uqk4GJ_sLHAcfjzYOzwfQwH1Mhto3aGhZyv2Vxm9B63IsY7hqkBBxvHfNzM9TWhvm6_0TeDW4Dhiovtjln2aFbVfEa9qJarUR76PnfQc_3eOv4p-gHSP_wyl9gM--8g75ClYR6NJmRBuSaEAz5mpIw", e).unwrap()),
    ]);

    // this gives us the header info - but can't "trust" any of this info as it is not part of the signed payload
    let passport_header_possible = decode_header(passport);

    if passport_header_possible.is_err() {
      return Err(HtsGetError::InvalidAuthentication(
        "needs Passport bearer token to be a correctly formatted JWT".to_string(),
      ));
    }

    let passport_header = passport_header_possible.unwrap();

    // we can't let the token chose *any* algorithm so we white-list the ones that are per-spec
    // TODO: support for ES256
    if passport_header.alg != RS256 {
      return Err(HtsGetError::InvalidAuthentication(
        "Passport bearer token needs to use algorithm RS256".to_string(),
      ));
    }

    // any we can insist on a token type
    if passport_header.typ.unwrap() != "vnd.ga4gh.passport+jwt" {
      return Err(HtsGetError::InvalidAuthentication(
        "Passport bearer token needs to be of type 'vnd.ga4gh.passport+jwt'".to_string(),
      ));
    }

    let mut allowed_access = false;

    // these are really just reflections of the above boolean - but are more precise to allow
    // us to return better error messages
    let mut found_trusted_broker = false;
    let mut found_trusted_visa = false;

    // we do not need to rely on header details like the 'kid' - we can speculatively attempt a decode against everyone we trust
    for b in brokers {
      let broker_validation = {
        let mut validation = Validation::new(passport_header.alg);
        validation.set_issuer(&[b.0]);
        validation
      };

      let passport_decoded_result =
        decode::<HashMap<String, serde_json::Value>>(passport, &b.1, &broker_validation);

      if passport_decoded_result.is_err() {
        let decode_err_string = passport_decoded_result.unwrap_err().to_string();

        // because we loop through *all* the brokers we actually expect InvalidSignature errors
        // we can just skip that token
        if decode_err_string.contains("InvalidSignature") {
          continue;
        }

        // however any other error is something we want to report
        // (in production we could skip - but we want this to show Expired tokens etc)
        return Err(HtsGetError::InvalidAuthentication(format!(
          "Passport bearer token did not successfully decode - it had the error {}",
          decode_err_string
        )));
      }

      found_trusted_broker = true;

      let passport_decoded = passport_decoded_result.unwrap();

      if passport_decoded.claims.contains_key("ga4gh_passport_v1") {
        let visa_array_value = passport_decoded.claims.get("ga4gh_passport_v1").unwrap();

        if visa_array_value.is_array() {
          let visa_array = visa_array_value.as_array();

          for possible_visa_string in visa_array.unwrap() {
            if possible_visa_string.is_string() {
              let visa_token = possible_visa_string.as_str().unwrap();

              let visa_header_possible = decode_header(visa_token);

              // skip invalid visas
              if visa_header_possible.is_err() {
                // TODO: log
                continue;
              }

              let visa_header = visa_header_possible.unwrap();

              // we can't let the token chose *any* algorithm so we white-list the ones that are per-spec
              if visa_header.alg != RS256 {
                continue;
              }

              for issuer in &issuers {
                let issuer_validation = {
                  let mut validation = Validation::new(visa_header.alg);
                  validation.set_issuer(&[issuer.0]);
                  validation
                };

                let visa_decoded_result = decode::<HashMap<String, serde_json::Value>>(
                  visa_token,
                  issuer.1,
                  &issuer_validation,
                );

                if visa_decoded_result.is_ok() {
                  found_trusted_visa = true;

                  let visa_decoded = visa_decoded_result.unwrap();

                  if visa_decoded.claims.contains_key("ga4gh_visa_v1") {
                    let grant_object = visa_decoded.claims.get("ga4gh_visa_v1").unwrap();

                    let grant_type = grant_object.get("type").unwrap().as_str().unwrap();

                    if grant_type == "ControlledAccessGrants" {
                      let grant_value = grant_object.get("value").unwrap().as_str().unwrap();

                      let grant_path: Vec<&str> = ega_regex
                        .captures_iter(grant_value)
                        .map(|caps| {
                          let (_, [ega_dataset]) = caps.extract();
                          ega_dataset
                        })
                        .collect();

                      // if the path and the visa "value" both mention the same EGA dataset then
                      // we allow access
                      if !grant_path.is_empty() && grant_path[0] == ega_path[0] {
                        allowed_access = true;
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }

    if !found_trusted_broker {
      return Err(HtsGetError::PermissionDenied(
        "Passport bearer token needs to be from an broker that we have a trust relationship with"
          .to_string(),
      ));
    }

    if !found_trusted_visa {
      return Err(HtsGetError::PermissionDenied(
        "Passport bearer token needs to contain a visa from a visa issuer that we have a trust relationship with".to_string(),
      ));
    }

    if !allowed_access {
      return Err(HtsGetError::PermissionDenied(
        "Visa from EGA not found allowing access to this EGA dataset".to_string(),
      ));
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
