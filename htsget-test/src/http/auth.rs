//! Authentication and authorization testing utils.

use crate::http::server::test_responses;
use crate::http::{Header, TestRequest, TestServer};
use axum::{http::StatusCode, response::Json, routing::get, Router};
use chrono::{Duration, Utc};
use htsget_config::config::advanced::auth::{
  AuthConfig, AuthMode, AuthorizationRestrictions, AuthorizationRule, ReferenceNameRestriction,
};
use htsget_config::config::advanced::HttpClient;
use htsget_config::types::{Class, Format, Interval};
use http::{Method, Uri};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header as JwtHeader};
use serde::Deserialize;
use serde_json::{json, Value};
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
pub fn create_test_auth_config(mock_server: &MockAuthServer, public_key: Vec<u8>) -> AuthConfig {
  AuthConfig::new(
    AuthMode::PublicKey(public_key),
    Some(vec!["test-audience".to_string()]),
    Some(vec!["test-issuer".to_string()]),
    Some("test-subject".to_string()),
    vec![mock_server.uri()],
    None,
    HttpClient::new(reqwest::Client::new()),
  )
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
  let reference_restriction = ReferenceNameRestriction::new(
    "chrM".to_string(),
    Some(Format::Vcf),
    Interval::new(Some(1000), Some(2000)),
  );
  let rule = AuthorizationRule::new(
    "/1-vcf/sample1-bcbio-cancer".to_string(),
    Some(vec![reference_restriction]),
  );
  AuthorizationRestrictions::new(1, vec![rule])
}

/// Test authorization with insufficient permissions.
pub async fn test_auth_insufficient_permissions<T: TestRequest>(
  tester: &impl TestServer<T>,
  private_key: Vec<u8>,
) {
  let claims = create_jwt_claims();
  let token = create_test_jwt_token(claims, private_key);

  let request = tester
    .request()
    .method(Method::GET)
    .uri("/variants/1-vcf/sample1-bcbio-cancer?referenceName=chrM&format=VCF&start=0&end=1500")
    .insert_header(Header {
      name: http::header::AUTHORIZATION,
      value: format!("Bearer {token}")
        .parse::<http::HeaderValue>()
        .unwrap(),
    });

  let response = tester
    .test_server(request, tester.get_expected_path().await)
    .await;
  assert_eq!(response.status, 403);
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

  test_responses::<R, T>(tester, vec![request], Class::Body).await
}
