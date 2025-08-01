//! Authentication and authorization testing utils.

use crate::http::{Header, TestRequest, TestServer};
use axum::{extract::State, http::StatusCode, response::Json, routing::get, Router};
use htsget_config::config::advanced::auth::{
  AuthConfig, AuthMode, AuthorizationRestrictions, AuthorizationRule, ReferenceNameRestriction,
};
use htsget_config::config::advanced::HttpClient;
use htsget_config::types::{Format, Interval};
use http::{Method, Uri};
use jsonwebtoken::{encode, EncodingKey, Header as JwtHeader};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/// Mock authorization server for testing JWT authorization flows.
pub struct MockAuthServer {
  addr: SocketAddr
}

impl MockAuthServer {
  /// Create a new mock authorization server.
  pub async fn new() -> Self {
    async fn auth_handler(
    ) -> Result<Json<AuthorizationRestrictions>, StatusCode> {
      Ok(Json(create_auth_restrictions()))
    }

    async fn jwks_handler() -> Result<Json<Value>, StatusCode> {
      Ok(Json(create_test_jwks()))
    }

    let app = Router::new()
      .route("/", get(auth_handler))
      .route("/.well-known/jwks.json", get(jwks_handler));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
      axum::serve(listener, app).await.unwrap();
    });

    Self {
      addr
    }
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
pub fn create_test_jwt_token(claims: Value) -> String {
  let header = JwtHeader::new(jsonwebtoken::Algorithm::RS256);
  let encoding_key =
    EncodingKey::from_rsa_pem(include_bytes!("../../../data/auth/private_key.pem")).unwrap();
  encode(&header, &claims, &encoding_key).unwrap()
}

/// Create a test auth config with JWKS mode.
pub fn create_test_auth_config_jwks(mock_server: &MockAuthServer) -> AuthConfig {
  AuthConfig::new(
    AuthMode::Jwks(mock_server.uri()),
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
    "exp": (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp(),
    "iat": chrono::Utc::now().timestamp(),
    "nbf": chrono::Utc::now().timestamp()
  })
}

/// Create authorization restrictions with specific reference name restrictions.
pub fn create_auth_restrictions() -> AuthorizationRestrictions {
  let reference_restriction = ReferenceNameRestriction::new(
    "chr1".to_string(),
    Some(Format::Bam),
    Interval::new(Some(1000), Some(2000)),
  );
  let rule = AuthorizationRule::new(
    "/1-vcf/sample1-bcbio-cancer".to_string(),
    Some(vec![reference_restriction]),
  );
  AuthorizationRestrictions::new(1, vec![rule])
}

/// Create a JWKS response for testing.
pub fn create_test_jwks() -> Value {
  json!({
    "keys": [{
      "kty": "RSA",
      "use": "sig",
      "kid": "test-key-id",
      "n": "test-modulus",
      "e": "AQAB"
    }]
  })
}

/// Test authorization with insufficient permissions.
pub async fn test_auth_insufficient_permissions<T: TestRequest>(tester: &impl TestServer<T>) {
  let claims = create_jwt_claims();
  let token = create_test_jwt_token(claims);

  let request = tester
    .request()
    .method(Method::GET)
    .uri("/variants/1-vcf/sample1-bcbio-cancer?referenceName=chr1&format=BAM&start=0&end=1500")
    .insert_header(Header {
      name: http::header::AUTHORIZATION,
      value: format!("Bearer {token}")
        .parse::<http::HeaderValue>()
        .unwrap(),
    });

  let response = tester.test_server(request, "".to_string()).await;
  assert_eq!(response.status, 403);
}

/// Run all authentication tests.
pub async fn test_all_auth<T: TestRequest>(tester: &impl TestServer<T>) {
  test_auth_insufficient_permissions(tester).await;
}
