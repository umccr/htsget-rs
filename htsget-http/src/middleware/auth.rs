//! The htsget authorization middleware.
//!

use crate::HtsGetError;
use crate::error::Result as HtsGetResult;
use crate::middleware::error::Error::AuthBuilderError;
use crate::middleware::error::Result;
use cfg_if::cfg_if;
use headers::authorization::Bearer;
use headers::{Authorization, Header};
use htsget_config::config::advanced::auth::{
  AuthConfig, AuthMode, AuthorizationRestrictions, AuthorizationRule,
};
use htsget_config::types::{Class, Interval, Query};
use http::{HeaderMap, Uri};
use jsonpath_rust::JsonPath;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, TokenData, Validation, decode, decode_header};
use regex::Regex;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::fmt::{Debug, Formatter};
use tracing::trace;

/// Builder the the authorization middleware.
#[derive(Default, Debug)]
pub struct AuthBuilder {
  config: Option<AuthConfig>,
}

impl AuthBuilder {
  /// Set the config.
  pub fn with_config(mut self, config: AuthConfig) -> Self {
    self.config = Some(config);
    self
  }

  /// Build the auth layer, ensures that the config sets the correct parameters.
  pub fn build(self) -> Result<Auth> {
    let Some(mut config) = self.config else {
      return Err(AuthBuilderError("missing config".to_string()));
    };

    if config.trusted_authorization_urls().is_empty() {
      return Err(AuthBuilderError(
        "at least one trusted authorization url must be set".to_string(),
      ));
    }
    if config.authorization_path().is_none() && config.trusted_authorization_urls().len() > 1 {
      return Err(AuthBuilderError(
        "only one trusted authorization url should be set when not using authorization paths"
          .to_string(),
      ));
    }

    let mut decoding_key = None;
    if let AuthMode::PublicKey(public_key) = config.auth_mode_mut() {
      decoding_key = Some(
        Auth::decode_public_key(public_key)
          .map_err(|_| AuthBuilderError("failed to decode public key".to_string()))?,
      );
    }

    Ok(Auth {
      config,
      decoding_key,
    })
  }
}

/// The auth middleware layer.
#[derive(Clone)]
pub struct Auth {
  config: AuthConfig,
  decoding_key: Option<DecodingKey>,
}

impl Debug for Auth {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("config").finish()
  }
}

impl Auth {
  /// Get the config for this auth layer instance.
  pub fn config(&self) -> &AuthConfig {
    &self.config
  }

  /// Fetch JWKS from the authorization server.
  pub async fn fetch_from_url<D: DeserializeOwned>(
    &self,
    url: &str,
    headers: HeaderMap,
  ) -> HtsGetResult<D> {
    trace!("fetching url: {}", url);
    let err = || HtsGetError::InternalError(format!("failed to fetch data from {url}"));
    let response = self
      .config
      .http_client()
      .get(url)
      .headers(headers)
      .send()
      .await
      .map_err(|_| err())?;

    response.json().await.map_err(|_| err())
  }

  /// Get a decoding key form the JWKS url.
  pub async fn decode_jwks(&self, jwks_url: &Uri, token: &str) -> HtsGetResult<DecodingKey> {
    // Decode header and get the key id.
    let header = decode_header(token)?;
    let kid = header
      .kid
      .ok_or_else(|| HtsGetError::PermissionDenied("JWT missing key ID".to_string()))?;

    // Fetch JWKS from the authorization server and find matching JWK.
    let jwks = self
      .fetch_from_url::<JwkSet>(&jwks_url.to_string(), Default::default())
      .await?;
    let matched_jwk = jwks
      .find(&kid)
      .ok_or_else(|| HtsGetError::PermissionDenied("matching JWK not found".to_string()))?;

    Ok(DecodingKey::from_jwk(matched_jwk)?)
  }

  /// Decode a public key into an RSA, EdDSA or ECDSA pem-formatted decoding key.
  pub fn decode_public_key(key: &[u8]) -> HtsGetResult<DecodingKey> {
    Ok(
      DecodingKey::from_rsa_pem(key)
        .or_else(|_| DecodingKey::from_ed_pem(key))
        .or_else(|_| DecodingKey::from_ec_pem(key))?,
    )
  }

  /// Query the authorization service to get the restrictions. This function validates
  /// that the authorization url is trusted in the config settings before calling the
  /// service. The claims are assumed to be valid.
  pub async fn query_authorization_service(
    &self,
    claims: Value,
    headers: &HeaderMap,
  ) -> HtsGetResult<AuthorizationRestrictions> {
    let query_url = match self.config.authorization_path() {
      None => self
        .config
        .trusted_authorization_urls()
        .first()
        .ok_or_else(|| {
          HtsGetError::InternalError("missing trusted authorization url".to_string())
        })?,
      Some(path) => {
        // Extract the url from the path.
        let path = claims.query(path).map_err(|err| {
          HtsGetError::InvalidAuthentication(format!(
            "failed to find authorization service in claims: {err}",
          ))
        })?;
        let url = path
          .first()
          .ok_or_else(|| {
            HtsGetError::InvalidAuthentication(
              "expected one value for authorization service in claims".to_string(),
            )
          })?
          .as_str()
          .ok_or_else(|| {
            HtsGetError::InvalidAuthentication(
              "expected string value for authorization service in claims".to_string(),
            )
          })?;
        &url.parse::<Uri>().map_err(|err| {
          HtsGetError::InvalidAuthentication(format!("failed to parse authorization url: {err}"))
        })?
      }
    };

    // Ensure that the authorization url is trusted.
    if !self.config.trusted_authorization_urls().contains(query_url) {
      return Err(HtsGetError::PermissionDenied(
        "authorization service in claims not a trusted authorization url".to_string(),
      ));
    };

    let auth_header = headers
      .iter()
      .find_map(|(name, value)| {
        if Authorization::<Bearer>::decode(&mut [value].into_iter()).is_ok() {
          return Some((name.clone(), value.clone()));
        }

        None
      })
      .ok_or_else(|| HtsGetError::PermissionDenied("missing authorization header".to_string()))?;

    self
      .fetch_from_url(&query_url.to_string(), HeaderMap::from_iter([auth_header]))
      .await
  }

  /// Validate the restrictions, returning an error if the user is not authorized.
  /// If `suppressed_interval` is set then no error is returning if there is a
  /// path match but no restrictions match. Instead, as many regions as possible
  /// are returned.
  pub fn validate_restrictions(
    restrictions: AuthorizationRestrictions,
    path: &str,
    queries: &mut [Query],
    suppressed_interval: bool,
  ) -> HtsGetResult<Vec<AuthorizationRule>> {
    // Find all rules matching the path.
    let matching_rules = restrictions
      .into_rules()
      .into_iter()
      .filter(|rule| {
        // If this path is a direct match then just return that.
        if rule.path().strip_prefix("/").unwrap_or(rule.path())
          == path.strip_prefix("/").unwrap_or(path)
        {
          return true;
        }

        // Otherwise, try and parse it as a regex.
        Regex::new(rule.path()).is_ok_and(|regex| regex.is_match(path))
      })
      .collect::<Vec<_>>();

    // If no paths match, then this is an authorization error.
    if matching_rules.is_empty() {
      return Err(HtsGetError::PermissionDenied(
        "failed to authorize user based on authorization service restrictions".to_string(),
      ));
    }

    let (allows_all, allows_specific): (Vec<_>, Vec<_>) = matching_rules
      .into_iter()
      .partition(|rule| rule.reference_names().is_none());

    // Otherwise, we need to check if the specific reference name is allowed for all queries.
    for query in queries {
      // If the request is for headers only, then this should always be allowed.
      if query.class() == Class::Header {
        continue;
      }

      let matching_restriction = allows_specific
        .iter()
        .flat_map(|rule| rule.reference_names().unwrap_or_default())
        .filter_map(|restriction| {
          // The reference name should match exactly.
          let name_match = Some(restriction.name()) == query.reference_name();
          // The format should match if it's defined, otherwise it allows any format.
          let format_match =
            restriction.format().is_none() || restriction.format() == Some(query.format());
          // The interval should match and be constrained, considering undefined start or end ranges.
          let interval_match = if suppressed_interval {
            restriction.interval().constraint_interval(query.interval())
          } else {
            restriction.interval().contains_interval(query.interval())
          };

          if let Some(interval_match) = interval_match {
            if name_match && format_match {
              return Some(interval_match);
            }
          }

          None
        })
        .max_by(Interval::order_by_range); // The largest interval should be used if there are multiple matches.

      if suppressed_interval {
        if allows_all.is_empty() && matching_restriction.is_none() {
          // If nothing allows all and there are no matching intervals then return an empty response.
          query.set_class(Class::Header);
          continue;
        }

        if let Some(matching_restriction) = matching_restriction {
          query.set_interval(matching_restriction);
        }
      } else if allows_all.is_empty() && matching_restriction.is_none() {
        return Err(HtsGetError::PermissionDenied(
          "failed to authorize user based on authorization service restrictions".to_string(),
        ));
      }
    }

    Ok([allows_all, allows_specific].concat())
  }

  /// Validate only the JWT without looking up restrictions and validating those. Returns the
  /// decoded JWT token.
  pub async fn validate_jwt(&self, headers: &HeaderMap) -> HtsGetResult<TokenData<Value>> {
    let auth_token = headers
      .values()
      .find_map(|value| Authorization::<Bearer>::decode(&mut [value].into_iter()).ok())
      .ok_or_else(|| {
        HtsGetError::InvalidAuthentication("invalid authorization header".to_string())
      })?;

    let decoding_key = if let Some(ref decoding_key) = self.decoding_key {
      decoding_key
    } else {
      match self.config.auth_mode() {
        AuthMode::Jwks(jwks) => &self.decode_jwks(jwks, auth_token.token()).await?,
        AuthMode::PublicKey(public_key) => &Self::decode_public_key(public_key)?,
      }
    };

    // Decode and validate the JWT
    let mut validation = Validation::default();
    validation.validate_exp = true;
    validation.validate_aud = true;
    validation.validate_nbf = true;

    if let Some(iss) = self.config.validate_issuer() {
      validation.set_issuer(iss);
      validation.required_spec_claims.insert("iss".to_string());
    }
    if let Some(aud) = self.config.validate_audience() {
      validation.set_audience(aud);
      validation.required_spec_claims.insert("aud".to_string());
    }
    if let Some(sub) = self.config.validate_subject() {
      validation.sub = Some(sub.to_string());
      validation.required_spec_claims.insert("sub".to_string());
    }

    // Each supported algorithm must be tried individually because the jsonwebtoken validation
    // logic only tries one algorithm in the vec: https://github.com/Keats/jsonwebtoken/issues/297
    validation.algorithms = vec![Algorithm::RS256];
    let decoded_claims = decode::<Value>(auth_token.token(), decoding_key, &validation)
      .or_else(|_| {
        validation.algorithms = vec![Algorithm::ES256];
        decode::<Value>(auth_token.token(), decoding_key, &validation)
      })
      .or_else(|_| {
        validation.algorithms = vec![Algorithm::EdDSA];
        decode::<Value>(auth_token.token(), decoding_key, &validation)
      });

    let claims = match decoded_claims {
      Ok(claims) => claims,
      Err(err) => return Err(HtsGetError::PermissionDenied(format!("invalid JWT: {err}"))),
    };

    Ok(claims)
  }

  /// Validate the authorization flow, returning an error if the user is not authorized.
  /// This performs the following steps:
  ///
  /// 1. Finds the JWT decoding key from the config or by querying a JWKS url.
  /// 2. Validates the JWT token according to the config.
  /// 3. Queries the authorization service for restrictions based on the config or JWT claims.
  /// 4. Validates the restrictions to determine if the user is authorized.
  pub async fn validate_authorization(
    &self,
    claims: TokenData<Value>,
    headers: &HeaderMap,
    path: &str,
    queries: &mut [Query],
  ) -> HtsGetResult<Vec<AuthorizationRule>> {
    let restrictions = self
      .query_authorization_service(claims.claims, headers)
      .await?;
    cfg_if! {
      if #[cfg(feature = "experimental")] {
        Self::validate_restrictions(restrictions, path, queries, self.config.suppress_errors())
      } else {
        Self::validate_restrictions(restrictions, path, queries, false)
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{Endpoint, convert_to_query, match_format_from_query};
  use htsget_config::config::advanced::HttpClient;
  use htsget_config::config::advanced::auth::response::{
    AuthorizationRestrictionsBuilder, AuthorizationRuleBuilder, ReferenceNameRestrictionBuilder,
  };
  use htsget_config::config::advanced::auth::{AuthConfigBuilder, AuthMode};
  use htsget_config::types::{Format, Request};
  use htsget_test::util::generate_key_pair;
  use http::{HeaderMap, Uri};
  use std::collections::HashMap;

  #[test]
  fn auth_builder_missing_config() {
    let result = AuthBuilder::default().build();
    assert!(matches!(result, Err(AuthBuilderError(_))));
  }

  #[test]
  fn auth_builder_success_with_public_key() {
    let (_, public_key) = generate_key_pair();

    let config = create_test_auth_config(public_key);
    let result = AuthBuilder::default().with_config(config).build();
    assert!(result.is_ok());
  }

  #[test]
  fn validate_restrictions_rule_allows_all() {
    let rule = AuthorizationRuleBuilder::default()
      .path("/reads/sample1")
      .build()
      .unwrap();
    let restrictions = AuthorizationRestrictionsBuilder::default()
      .rule(rule)
      .build()
      .unwrap();

    let request = create_test_query(Endpoint::Reads, "/reads/sample1", HashMap::new());
    let result =
      Auth::validate_restrictions(restrictions, request.id(), &mut [request.clone()], false);
    assert!(result.is_ok());
  }

  #[test]
  fn validate_restrictions_exact_path_match() {
    let reference_restriction = ReferenceNameRestrictionBuilder::default()
      .name("chr1")
      .format(Format::Bam)
      .start(1000)
      .end(2000)
      .build()
      .unwrap();
    let rule = AuthorizationRuleBuilder::default()
      .path("/reads/sample1")
      .reference_name(reference_restriction)
      .build()
      .unwrap();
    let restrictions = AuthorizationRestrictionsBuilder::default()
      .rule(rule)
      .build()
      .unwrap();

    let mut query = HashMap::new();
    query.insert("referenceName".to_string(), "chr1".to_string());
    query.insert("start".to_string(), "1500".to_string());
    query.insert("end".to_string(), "1800".to_string());
    query.insert("format".to_string(), "BAM".to_string());

    let request = create_test_query(Endpoint::Reads, "/reads/sample1", query);
    let result =
      Auth::validate_restrictions(restrictions, request.id(), &mut [request.clone()], false);
    assert!(result.is_ok());
  }

  #[test]
  fn validate_restrictions_regex_path_match() {
    let reference_restriction = ReferenceNameRestrictionBuilder::default()
      .name("chr1")
      .format(Format::Bam)
      .build()
      .unwrap();
    let rule = AuthorizationRuleBuilder::default()
      .path("/reads/sample(.+)")
      .reference_name(reference_restriction)
      .build()
      .unwrap();
    let restrictions = AuthorizationRestrictionsBuilder::default()
      .rule(rule)
      .build()
      .unwrap();

    let mut query = HashMap::new();
    query.insert("referenceName".to_string(), "chr1".to_string());
    query.insert("format".to_string(), "BAM".to_string());

    let request = create_test_query(Endpoint::Reads, "/reads/sample123", query);
    let result =
      Auth::validate_restrictions(restrictions, request.id(), &mut [request.clone()], false);
    assert!(result.is_ok());
  }

  #[test]
  fn validate_restrictions_reference_name_mismatch() {
    let reference_restriction = ReferenceNameRestrictionBuilder::default()
      .name("chr1")
      .format(Format::Bam)
      .build()
      .unwrap();
    let rule = AuthorizationRuleBuilder::default()
      .path("/reads/sample1")
      .reference_name(reference_restriction)
      .build()
      .unwrap();
    let restrictions = AuthorizationRestrictionsBuilder::default()
      .rule(rule.clone())
      .build()
      .unwrap();

    let mut query = HashMap::new();
    query.insert("class".to_string(), "header".to_string());
    query.insert("format".to_string(), "BAM".to_string());

    let request = create_test_query(Endpoint::Reads, "/reads/sample1", query);
    let result =
      Auth::validate_restrictions(restrictions, request.id(), &mut [request.clone()], false);
    assert!(result.is_ok());
  }

  #[test]
  fn validate_restrictions_header() {
    let reference_restriction = ReferenceNameRestrictionBuilder::default()
      .name("chr1")
      .format(Format::Bam)
      .build()
      .unwrap();
    let rule = AuthorizationRuleBuilder::default()
      .path("/reads/sample1")
      .reference_name(reference_restriction)
      .build()
      .unwrap();
    let restrictions = AuthorizationRestrictionsBuilder::default()
      .rule(rule.clone())
      .build()
      .unwrap();

    let mut query = HashMap::new();
    query.insert("referenceName".to_string(), "chr2".to_string());
    query.insert("format".to_string(), "BAM".to_string());

    let request = create_test_query(Endpoint::Reads, "/reads/sample1", query);
    let result =
      Auth::validate_restrictions(restrictions, request.id(), &mut [request.clone()], false);
    assert!(result.is_err());
  }

  #[cfg(feature = "experimental")]
  #[test]
  fn validate_restrictions_reference_name_mismatch_suppressed() {
    let reference_restriction = ReferenceNameRestrictionBuilder::default()
      .name("chr1")
      .format(Format::Bam)
      .build()
      .unwrap();
    let rule = AuthorizationRuleBuilder::default()
      .path("/reads/sample1")
      .reference_name(reference_restriction)
      .build()
      .unwrap();
    let restrictions = AuthorizationRestrictionsBuilder::default()
      .rule(rule.clone())
      .build()
      .unwrap();

    let mut query = HashMap::new();
    query.insert("referenceName".to_string(), "chr2".to_string());
    query.insert("format".to_string(), "BAM".to_string());

    let request = create_test_query(Endpoint::Reads, "/reads/sample1", query);
    let result =
      Auth::validate_restrictions(restrictions, request.id(), &mut [request.clone()], true);
    assert!(result.is_ok());
  }

  #[test]
  fn validate_restrictions_format_mismatch() {
    let reference_restriction = ReferenceNameRestrictionBuilder::default()
      .name("chr1")
      .format(Format::Bam)
      .build()
      .unwrap();
    let rule = AuthorizationRuleBuilder::default()
      .path("/reads/sample1")
      .reference_name(reference_restriction)
      .build()
      .unwrap();
    let restrictions = AuthorizationRestrictionsBuilder::default()
      .rule(rule.clone())
      .build()
      .unwrap();

    let mut query = HashMap::new();
    query.insert("referenceName".to_string(), "chr1".to_string());
    query.insert("format".to_string(), "CRAM".to_string());

    let request = create_test_query(Endpoint::Reads, "/reads/sample1", query);
    let result =
      Auth::validate_restrictions(restrictions, request.id(), &mut [request.clone()], false);
    assert!(result.is_err());
  }

  #[cfg(feature = "experimental")]
  #[test]
  fn validate_restrictions_format_mismatch_suppressed() {
    let reference_restriction = ReferenceNameRestrictionBuilder::default()
      .name("chr1")
      .format(Format::Bam)
      .build()
      .unwrap();
    let rule = AuthorizationRuleBuilder::default()
      .path("/reads/sample1")
      .reference_name(reference_restriction)
      .build()
      .unwrap();
    let restrictions = AuthorizationRestrictionsBuilder::default()
      .rule(rule.clone())
      .build()
      .unwrap();

    let mut query = HashMap::new();
    query.insert("referenceName".to_string(), "chr1".to_string());
    query.insert("format".to_string(), "CRAM".to_string());

    let request = create_test_query(Endpoint::Reads, "/reads/sample1", query);
    let result =
      Auth::validate_restrictions(restrictions, request.id(), &mut [request.clone()], true);
    assert!(result.is_ok());
  }

  #[test]
  fn validate_restrictions_interval_not_contained() {
    let reference_restriction = ReferenceNameRestrictionBuilder::default()
      .name("chr1")
      .format(Format::Bam)
      .start(1000)
      .end(2000)
      .build()
      .unwrap();
    let rule = AuthorizationRuleBuilder::default()
      .path("/reads/sample1")
      .reference_name(reference_restriction)
      .build()
      .unwrap();
    let restrictions = AuthorizationRestrictionsBuilder::default()
      .rule(rule.clone())
      .build()
      .unwrap();

    let mut query = HashMap::new();
    query.insert("referenceName".to_string(), "chr1".to_string());
    query.insert("start".to_string(), "500".to_string());
    query.insert("end".to_string(), "1500".to_string());

    let request = create_test_query(Endpoint::Reads, "/reads/sample1", query);
    let result =
      Auth::validate_restrictions(restrictions, request.id(), &mut [request.clone()], false);
    assert!(result.is_err());
  }

  #[cfg(feature = "experimental")]
  #[test]
  fn validate_restrictions_interval_not_contained_suppressed() {
    let reference_restriction = ReferenceNameRestrictionBuilder::default()
      .name("chr1")
      .format(Format::Bam)
      .start(1000)
      .end(2000)
      .build()
      .unwrap();
    let rule = AuthorizationRuleBuilder::default()
      .path("/reads/sample1")
      .reference_name(reference_restriction)
      .build()
      .unwrap();
    let restrictions = AuthorizationRestrictionsBuilder::default()
      .rule(rule.clone())
      .build()
      .unwrap();

    let mut query = HashMap::new();
    query.insert("referenceName".to_string(), "chr1".to_string());
    query.insert("start".to_string(), "500".to_string());
    query.insert("end".to_string(), "1500".to_string());

    let request = create_test_query(Endpoint::Reads, "/reads/sample1", query);
    let result =
      Auth::validate_restrictions(restrictions, request.id(), &mut [request.clone()], true);
    assert!(result.is_ok())
  }

  #[test]
  fn validate_restrictions_format_none_allows_any() {
    let reference_restriction = ReferenceNameRestrictionBuilder::default()
      .name("chr1")
      .build()
      .unwrap();
    let rule = AuthorizationRuleBuilder::default()
      .path("/reads/sample1")
      .reference_name(reference_restriction)
      .build()
      .unwrap();
    let restrictions = AuthorizationRestrictionsBuilder::default()
      .rule(rule)
      .build()
      .unwrap();

    let mut query = HashMap::new();
    query.insert("referenceName".to_string(), "chr1".to_string());
    query.insert("format".to_string(), "CRAM".to_string());

    let request = create_test_query(Endpoint::Reads, "/reads/sample1", query);
    let result =
      Auth::validate_restrictions(restrictions, request.id(), &mut [request.clone()], false);
    assert!(result.is_ok());
  }

  #[test]
  fn validate_restrictions_path_with_leading_slash() {
    let rule = AuthorizationRuleBuilder::default()
      .path("/reads/sample1")
      .build()
      .unwrap();
    let restrictions = AuthorizationRestrictionsBuilder::default()
      .rule(rule)
      .build()
      .unwrap();
    let request = create_test_query(Endpoint::Reads, "/reads/sample1", HashMap::new());
    let result =
      Auth::validate_restrictions(restrictions, request.id(), &mut [request.clone()], false);
    assert!(result.is_ok());
  }

  #[tokio::test]
  async fn validate_authorization_missing_auth_header() {
    let auth = create_mock_auth_with_restrictions();
    let request = Request::new(
      "/reads/sample1".to_string(),
      HashMap::new(),
      HeaderMap::new(),
    );

    let result = auth.validate_jwt(request.headers()).await;
    assert!(result.is_err());
    assert!(matches!(
      result.unwrap_err(),
      HtsGetError::InvalidAuthentication(_)
    ));
  }

  #[tokio::test]
  async fn validate_authorization_invalid_jwt_format() {
    let auth = create_mock_auth_with_restrictions();
    let request =
      create_request_with_auth_header("/reads/sample1", HashMap::new(), "invalid.jwt.token");

    let result = auth.validate_jwt(request.headers()).await;
    assert!(result.is_err());
    assert!(matches!(
      result.unwrap_err(),
      HtsGetError::PermissionDenied(_)
    ));
  }

  fn create_test_auth_config(public_key: Vec<u8>) -> AuthConfig {
    AuthConfigBuilder::default()
      .auth_mode(AuthMode::PublicKey(public_key))
      .trusted_authorization_url(Uri::from_static("https://www.example.com"))
      .http_client(HttpClient::new(reqwest::Client::new()))
      .build()
      .unwrap()
  }

  fn create_test_query(endpoint: Endpoint, path: &str, query: HashMap<String, String>) -> Query {
    let request = Request::new(path.to_string(), query, HeaderMap::new());
    let format = match_format_from_query(&endpoint, request.query()).unwrap();

    convert_to_query(request, format).unwrap()
  }

  fn create_request_with_auth_header(
    path: &str,
    query: HashMap<String, String>,
    token: &str,
  ) -> Request {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
    Request::new(path.to_string(), query, headers)
  }

  fn create_mock_auth_with_restrictions() -> Auth {
    let (_, public_key) = generate_key_pair();

    let config = create_test_auth_config(public_key);
    AuthBuilder::default().with_config(config).build().unwrap()
  }
}
