//! Authentication and authorization testing utils.

use crate::http::server::test_responses;
use crate::http::{Header, TestRequest, TestServer};
use axum::{Router, http::StatusCode, response::Json, routing::get};
use cfg_if::cfg_if;
use chrono::{Duration, Utc};
use htsget_config::config::advanced::HttpClient;
use htsget_config::config::advanced::auth::response::{
  AuthorizationRestrictionsBuilder, AuthorizationRuleBuilder, ReferenceNameRestrictionBuilder,
};
use htsget_config::config::advanced::auth::{
  AuthConfig, AuthConfigBuilder, AuthMode, AuthorizationRestrictions,
};
use htsget_config::types::{Class, Format};
use http::{Method, Uri};
use jsonwebtoken::{Algorithm, EncodingKey, Header as JwtHeader, encode};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fmt::Debug;
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Mock authorization server for testing JWT authorization flows.
pub struct MockAuthServer {
  addr: SocketAddr,
}

impl MockAuthServer {
  /// Create a new mock authorization server.
  pub async fn new() -> Self {
    async fn auth_handler() -> Result<Json<AuthorizationRestrictions>, StatusCode> {
      Ok(Json(create_auth_restrictions()))
    }

    let app = Router::new().route("/", get(auth_handler));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
      axum::serve(listener, app).await.unwrap();
    });

    Self { addr }
  }

  /// Get the server URL.
  pub fn url(&self) -> String {
    format!("http://{}", self.addr)
  }

  /// Get the server URI.
  pub fn uri(&self) -> Uri {
    self.url().parse::<Uri>().unwrap()
  }
}

/// Create a test JWT token with the given claims.
pub fn create_test_jwt_token(claims: Value, private_key: Vec<u8>) -> String {
  let header = JwtHeader::new(Algorithm::ES256);
  let encoding_key = EncodingKey::from_ec_pem(&private_key).unwrap();
  encode(&header, &claims, &encoding_key).unwrap()
}

/// Create a test auth config with JWKS mode.
pub fn create_test_auth_config(
  mock_server: &MockAuthServer,
  public_key: Vec<u8>,
  _suppressed: bool,
) -> AuthConfig {
  let builder = AuthConfigBuilder::default()
    .auth_mode(AuthMode::PublicKey(public_key))
    .validate_audience(vec!["test-audience".to_string()])
    .validate_issuer(vec!["test-issuer".to_string()])
    .validate_subject("test-subject".to_string())
    .trusted_authorization_url(mock_server.uri())
    .http_client(HttpClient::new(reqwest::Client::new()));

  cfg_if! {
    if #[cfg(feature = "experimental")] {
      builder.suppress_errors(_suppressed)
      .add_hint(_suppressed)
      .build().unwrap()
    } else {
      builder.build().unwrap()
    }
  }
}

/// Create a valid JWT token for testing.
pub fn create_jwt_claims() -> Value {
  json!({
    "iss": "test-issuer",
    "aud": "test-audience",
    "sub": "test-subject",
    "exp": (Utc::now() + Duration::hours(1)).timestamp()
  })
}

/// Create authorization restrictions with specific reference name restrictions.
pub fn create_auth_restrictions() -> AuthorizationRestrictions {
  AuthorizationRestrictionsBuilder::default()
    .version(1)
    .rule(
      AuthorizationRuleBuilder::default()
        .path("/1-vcf/sample1-bcbio-cancer")
        .reference_name(
          ReferenceNameRestrictionBuilder::default()
            .name("chrM")
            .format(Format::Vcf)
            .start(1000)
            .end(2000)
            .build()
            .unwrap(),
        )
        .build()
        .unwrap(),
    )
    .build()
    .unwrap()
}

/// Test authorization with insufficient permissions.
pub async fn test_auth_insufficient_permissions<R, T>(
  tester: &impl TestServer<T>,
  private_key: Vec<u8>,
) where
  T: TestRequest,
  R: for<'de> Deserialize<'de> + Eq + Debug,
{
  let claims = create_jwt_claims();
  let token = create_test_jwt_token(claims, private_key);

  let request = tester
    .request()
    .method(Method::GET)
    .uri("/variants/1-vcf/sample1-bcbio-cancer?referenceName=chrM&format=VCF&start=0&end=900")
    .insert_header(Header {
      name: http::header::AUTHORIZATION,
      value: format!("Bearer {token}")
        .parse::<http::HeaderValue>()
        .unwrap(),
    });

  if tester_suppress_errors(tester) {
    let additional_fields = HashMap::from_iter(vec![(
      "allowed".to_string(),
      serde_json::to_value(create_auth_restrictions().htsget_auth()).unwrap(),
    )]);
    test_responses::<R, T>(tester, vec![request], Class::Header, additional_fields).await
  } else {
    let response = tester
      .test_server(request, tester.get_expected_path().await)
      .await;
    assert_eq!(response.status, 403);
  }
}

/// Test authorization that should succeed.
pub async fn test_auth_succeeds<R, T>(tester: &impl TestServer<T>, private_key: Vec<u8>)
where
  T: TestRequest,
  R: for<'de> Deserialize<'de> + Eq + Debug,
{
  let claims = create_jwt_claims();
  let token = create_test_jwt_token(claims, private_key);

  let request = tester
    .request()
    .method(Method::GET)
    .uri("/variants/1-vcf/sample1-bcbio-cancer?referenceName=chrM&format=VCF&start=1500&end=1700")
    .insert_header(Header {
      name: http::header::AUTHORIZATION,
      value: format!("Bearer {token}")
        .parse::<http::HeaderValue>()
        .unwrap(),
    });

  if tester_suppress_errors(tester) {
    let additional_fields = HashMap::from_iter(vec![(
      "allowed".to_string(),
      serde_json::to_value(create_auth_restrictions().htsget_auth()).unwrap(),
    )]);
    test_responses::<R, T>(tester, vec![request], Class::Body, additional_fields).await
  } else {
    test_responses::<R, T>(tester, vec![request], Class::Body, Default::default()).await
  }
}

fn tester_suppress_errors<T>(tester: &impl TestServer<T>) -> bool
where
  T: TestRequest,
{
  tester
    .get_config()
    .ticket_server()
    .auth()
    .is_some_and(|_auth| {
      cfg_if! {
        if #[cfg(feature = "experimental")] {
          _auth.suppress_errors()
        } else {
          false
        }
      }
    })
}
