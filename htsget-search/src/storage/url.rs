use std::fmt::Debug;
use std::num::ParseIntError;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::MapErr;
use futures_util::TryStreamExt;
use http::header::CONTENT_LENGTH;
use http::{HeaderMap, Method, Request, Response, Uri};
use hyper::client::HttpConnector;
use hyper::{body, Body, Client, Error};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use tokio_util::io::StreamReader;
use tracing::{debug, instrument};

use htsget_config::error;
use htsget_config::types::KeyType;

use crate::storage::StorageError::{InternalError, KeyNotFound, ResponseError, UrlParseError};
use crate::storage::{
  BytesPosition, BytesPositionOptions, GetOptions, HeadOptions, RangeUrlOptions, Result, Storage,
  StorageError,
};
use crate::Url as HtsGetUrl;

/// A storage struct which derives data from HTTP URLs.
#[derive(Debug, Clone)]
pub struct UrlStorage {
  client: Client<HttpsConnector<HttpConnector>>,
  endpoint_head: Uri,
  endpoint_file: Uri,
  endpoint_index: Uri,
  response_url: Uri,
  forward_headers: bool,
  #[cfg(feature = "crypt4gh")]
  endpoint_crypt4gh_header: Option<Uri>,
}

impl UrlStorage {
  /// Construct a new UrlStorage.
  pub fn new(
    client: Client<HttpsConnector<HttpConnector>>,
    endpoint_head: Uri,
    endpoint_file: Uri,
    endpoint_index: Uri,
    response_url: Uri,
    forward_headers: bool,
    #[cfg(feature = "crypt4gh")] endpoint_crypt4gh_header: Option<Uri>,
  ) -> Self {
    Self {
      client,
      endpoint_head,
      endpoint_file,
      endpoint_index,
      response_url,
      forward_headers,
      #[cfg(feature = "crypt4gh")]
      endpoint_crypt4gh_header,
    }
  }

  /// Construct a new UrlStorage with a default client.
  pub fn new_with_default_client(
    endpoint_head: Uri,
    endpoint_header: Uri,
    endpoint_index: Uri,
    response_url: Uri,
    forward_headers: bool,
    #[cfg(feature = "crypt4gh")] endpoint_crypt4gh_header: Option<Uri>,
  ) -> Self {
    Self {
      client: Client::builder().build(
        HttpsConnectorBuilder::new()
          .with_native_roots()
          .https_or_http()
          .enable_http1()
          .enable_http2()
          .build(),
      ),
      endpoint_head,
      endpoint_file: endpoint_header,
      endpoint_index,
      response_url,
      forward_headers,
      #[cfg(feature = "crypt4gh")]
      endpoint_crypt4gh_header,
    }
  }

  /// Get a url from the key.
  pub fn get_url_from_key<K: AsRef<str> + Send>(&self, key: K, endpoint: &Uri) -> Result<Uri> {
    let uri = if endpoint.to_string().ends_with("/") {
      format!("{}{}", endpoint, key.as_ref())
    } else {
      format!("{}/{}", endpoint, key.as_ref())
    };

    uri
      .parse::<Uri>()
      .map_err(|err| UrlParseError(err.to_string()))
  }

  /// Construct and send a request
  pub async fn send_request<K: AsRef<str> + Send>(
    &self,
    key: K,
    headers: &HeaderMap,
    method: Method,
    url: &Uri,
  ) -> Result<Response<Body>> {
    let key = key.as_ref();
    let url = self.get_url_from_key(key, url)?;

    let request = Request::builder().method(method).uri(&url);

    let request = headers
      .iter()
      .fold(request, |acc, (key, value)| acc.header(key, value))
      .body(Body::empty())
      .map_err(|err| UrlParseError(err.to_string()))?;

    let response = self
      .client
      .request(request)
      .await
      .map_err(|err| KeyNotFound(format!("{} with key {}", err, key)))?;

    let status = response.status();
    if status.is_client_error() || status.is_server_error() {
      Err(KeyNotFound(format!(
        "url returned {} for key {}",
        status, key
      )))
    } else {
      Ok(response)
    }
  }

  /// Construct and send a request
  pub async fn format_url<K: AsRef<str> + Send>(
    &self,
    key: K,
    options: RangeUrlOptions<'_>,
    endpoint: &Uri,
  ) -> Result<HtsGetUrl> {
    let key = key.as_ref();
    let url = self.get_url_from_key(key, endpoint)?.into_parts();
    let url = Uri::from_parts(url)
      .map_err(|err| InternalError(format!("failed to convert to uri from parts: {}", err)))?;

    let mut url = HtsGetUrl::new(url.to_string());
    if self.forward_headers {
      url = url.with_headers(
        options
          .response_headers()
          .try_into()
          .map_err(|err: error::Error| StorageError::InvalidInput(err.to_string()))?,
      )
    }

    Ok(options.apply(url))
  }

  /// Get the head from the key.
  pub async fn head_key<K: AsRef<str> + Send>(
    &self,
    key: K,
    headers: &HeaderMap,
  ) -> Result<Response<Body>> {
    self
      .send_request(key, headers, Method::HEAD, &self.endpoint_head)
      .await
  }

  /// Get the key.
  pub async fn get_header<K: AsRef<str> + Send>(
    &self,
    key: K,
    headers: &HeaderMap,
  ) -> Result<Response<Body>> {
    self
      .send_request(key, headers, Method::GET, &self.endpoint_file)
      .await
  }

  /// Get the key.
  pub async fn get_index<K: AsRef<str> + Send>(
    &self,
    key: K,
    headers: &HeaderMap,
  ) -> Result<Response<Body>> {
    self
      .send_request(key, headers, Method::GET, &self.endpoint_index)
      .await
  }
}

#[async_trait]
impl Storage for UrlStorage {
  type Streamable = StreamReader<MapErr<Body, fn(Error) -> StorageError>, Bytes>;

  #[instrument(level = "trace", skip(self))]
  async fn get<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    options: GetOptions<'_>,
  ) -> Result<Self::Streamable> {
    let key = key.as_ref().to_string();
    debug!(calling_from = ?self, key, "getting file with key {:?}", key);

    let response = match KeyType::from_ending(&key) {
      KeyType::File => {
        self
          .get_header(key.to_string(), options.request_headers())
          .await?
      }
      KeyType::Index => {
        self
          .get_index(key.to_string(), options.request_headers())
          .await?
      }
    };

    Ok(StreamReader::new(response.into_body().map_err(|err| {
      ResponseError(format!("reading body from response: {}", err))
    })))
  }

  #[instrument(level = "trace", skip(self))]
  async fn range_url<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    options: RangeUrlOptions<'_>,
  ) -> Result<HtsGetUrl> {
    let key = key.as_ref();
    debug!(calling_from = ?self, key, "getting url with key {:?}", key);

    self.format_url(key, options, &self.response_url).await
  }

  #[instrument(level = "trace", skip(self))]
  async fn head<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    options: HeadOptions<'_>,
  ) -> Result<u64> {
    let key = key.as_ref();
    let head = self.head_key(key, options.request_headers()).await?;

    let len = head
      .headers()
      .get(CONTENT_LENGTH)
      .and_then(|content_length| content_length.to_str().ok())
      .and_then(|content_length| content_length.parse().ok())
      .ok_or_else(|| {
        ResponseError(format!(
          "failed to get content length from head response for key: {}",
          key
        ))
      })?;

    debug!(calling_from = ?self, key, len, "size of key {:?} is {}", key, len);
    Ok(len)
  }

  #[instrument(level = "trace", skip(self))]
  async fn update_byte_positions<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    positions_options: BytesPositionOptions<'_>,
  ) -> Result<Vec<BytesPosition>> {
    let mut positions_options = positions_options;
    #[cfg(feature = "crypt4gh")]
    if let Some(endpoint_crypt4gh_header) = &self.endpoint_crypt4gh_header {
      let response = body::to_bytes(
        self
          .send_request(
            key,
            positions_options.headers(),
            Method::GET,
            endpoint_crypt4gh_header,
          )
          .await?
          .into_body(),
      )
      .await
      .map_err(|err| ResponseError(err.to_string()))?;

      let header_length: u64 = String::from_utf8(response.to_vec())
        .map_err(|err| ResponseError(err.to_string()))?
        .parse()
        .map_err(|err: ParseIntError| ResponseError(err.to_string()))?;

      let file_size = positions_options.file_size();
      positions_options = positions_options.convert_to_crypt4gh_ranges(header_length, file_size);
    }

    Ok(positions_options.merge_all().into_inner())
  }
}

#[cfg(test)]
mod tests {
  use std::future::Future;
  use std::io::Cursor;
  use std::net::TcpListener;
  use std::path::Path;
  use std::result;
  use std::str::FromStr;

  use axum::middleware::Next;
  use axum::response::{IntoResponse, Response};
  use axum::{middleware, Router};
  use axum::body::StreamBody;
  use axum::routing::{get, head};
  use http::header::AUTHORIZATION;
  use http::{HeaderName, HeaderValue, Request, StatusCode};
  use hyper::body::to_bytes;
  use noodles::{bam, sam};
  use crate::Response as HtsgetResponse;
  use tokio::fs::File;
  use tokio::io::AsyncReadExt;
  use tokio_util::io::ReaderStream;
  use tower_http::services::ServeDir;

  use htsget_config::types::{Format, Headers, Query, Url};
  use htsget_config::types::Class::{Body, Header};
  use crate::htsget::from_storage::HtsGetFromStorage;
  use crate::htsget::HtsGet;

  use crate::storage::local::tests::create_local_test_files;

  use super::*;

  #[test]
  fn get_url_from_key() {
    let storage = UrlStorage::new(
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://localhost:8080").unwrap(),
      true,
      #[cfg(feature = "crypt4gh")]
      None,
    );

    assert_eq!(
      storage
        .get_url_from_key(
          "assets/key1",
          &Uri::from_str("https://example.com").unwrap()
        )
        .unwrap(),
      Uri::from_str("https://example.com/assets/key1").unwrap()
    );
  }

  #[test]
  fn get_response_url_from_key() {
    let storage = UrlStorage::new(
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://localhost:8080").unwrap(),
      true,
      #[cfg(feature = "crypt4gh")]
      None,
    );

    assert_eq!(
      storage
        .get_url_from_key(
          "assets/key1",
          &Uri::from_str("https://localhost:8080").unwrap()
        )
        .unwrap(),
      Uri::from_str("https://localhost:8080/assets/key1").unwrap()
    );
  }

  #[tokio::test]
  async fn send_request() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        true,
        #[cfg(feature = "crypt4gh")]
        None,
      );

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);

      let response = String::from_utf8(
        to_bytes(
          storage
            .send_request(
              "assets/key1",
              headers,
              Method::GET,
              &Uri::from_str(&url).unwrap(),
            )
            .await
            .unwrap()
            .into_body(),
        )
        .await
        .unwrap()
        .to_vec(),
      )
      .unwrap();
      assert_eq!(response, "value1");
    })
    .await;
  }

  #[tokio::test]
  async fn get_key() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        true,
        #[cfg(feature = "crypt4gh")]
        None,
      );

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);

      let response = String::from_utf8(
        to_bytes(
          storage
            .get_header("assets/key1", headers)
            .await
            .unwrap()
            .into_body(),
        )
        .await
        .unwrap()
        .to_vec(),
      )
      .unwrap();
      assert_eq!(response, "value1");
    })
    .await;
  }

  #[tokio::test]
  async fn head_key() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        true,
        #[cfg(feature = "crypt4gh")]
        None,
      );

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);

      let response: u64 = storage
        .get_header("assets/key1", headers)
        .await
        .unwrap()
        .headers()
        .get(CONTENT_LENGTH)
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
      assert_eq!(response, 6);
    })
    .await;
  }

  #[tokio::test]
  async fn get_storage() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        true,
        #[cfg(feature = "crypt4gh")]
        None,
      );

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);
      let options = GetOptions::new_with_default_range(headers);

      let mut reader = storage.get("assets/key1", options).await.unwrap();

      let mut response = [0; 6];
      reader.read_exact(&mut response).await.unwrap();

      assert_eq!(String::from_utf8(response.to_vec()).unwrap(), "value1");
    })
    .await;
  }

  #[tokio::test]
  async fn range_url_storage() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        true,
        #[cfg(feature = "crypt4gh")]
        None,
      );

      let mut headers = HeaderMap::default();
      let options = test_range_options(&mut headers);

      assert_eq!(
        storage.range_url("assets/key1", options).await.unwrap(),
        HtsGetUrl::new(format!("{}/assets/key1", url))
          .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
      );
    })
    .await;
  }

  #[tokio::test]
  async fn head_storage() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        test_client(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        Uri::from_str(&url).unwrap(),
        true,
        #[cfg(feature = "crypt4gh")]
        None,
      );

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);
      let options = HeadOptions::new(headers);

      assert_eq!(storage.head("assets/key1", options).await.unwrap(), 6);
    })
    .await;
  }

  #[tokio::test]
  async fn format_url() {
    let storage = UrlStorage::new(
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://localhost:8080").unwrap(),
      true,
      #[cfg(feature = "crypt4gh")]
      None,
    );

    let mut headers = HeaderMap::default();
    let options = test_range_options(&mut headers);

    assert_eq!(
      storage
        .format_url(
          "assets/key1",
          options,
          &Uri::from_str("https://localhost:8080").unwrap()
        )
        .await
        .unwrap(),
      HtsGetUrl::new("https://localhost:8080/assets/key1")
        .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
    );
  }

  #[tokio::test]
  async fn format_url_different_response_scheme() {
    let storage = UrlStorage::new(
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("http://example.com").unwrap(),
      true,
      #[cfg(feature = "crypt4gh")]
      None,
    );

    let mut headers = HeaderMap::default();
    let options = test_range_options(&mut headers);

    assert_eq!(
      storage
        .format_url(
          "assets/key1",
          options,
          &Uri::from_str("http://example.com").unwrap()
        )
        .await
        .unwrap(),
      HtsGetUrl::new("http://example.com/assets/key1")
        .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
    );
  }

  #[tokio::test]
  async fn format_url_no_headers() {
    let storage = UrlStorage::new(
      test_client(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://example.com").unwrap(),
      Uri::from_str("https://localhost:8081").unwrap(),
      false,
      #[cfg(feature = "crypt4gh")]
      None,
    );

    let mut headers = HeaderMap::default();
    let options = test_range_options(&mut headers);

    assert_eq!(
      storage.range_url("assets/key1", options,).await.unwrap(),
      HtsGetUrl::new("https://localhost:8081/assets/key1")
    );
  }

  #[tokio::test]
  async fn test_endpoints_with_real_file() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        test_client(),
        Uri::from_str(&format!("{}/endpoint_head", url)).unwrap(),
        Uri::from_str(&format!("{}/endpoint_file", url)).unwrap(),
        Uri::from_str(&format!("{}/endpoint_index", url)).unwrap(),
        Uri::from_str("http://example.com").unwrap(),
        true,
        #[cfg(feature = "crypt4gh")]
          Some(Uri::from_str(&format!("{}/endpoint_crypt4gh_header", url)).unwrap()),
      );

      let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam)
        .with_reference_name("11")
        .with_start(5015000)
        .with_end(5050000);
      let searcher = HtsGetFromStorage::new(storage);
      let response = searcher.search(query.clone()).await;

      let expected_response = Ok(HtsgetResponse::new(
        Format::Bam,
        vec![
          Url::new("http://example.com/htsnexus_test_NA12878.bam")
            .with_headers(Headers::default().with_header("Range", "bytes=0-4667"))
            .with_class(Header),
          Url::new("http://example.com/htsnexus_test_NA12878.bam")
            .with_headers(Headers::default().with_header("Range", "bytes=256721-1065951"))
            .with_class(Body),
          Url::new("http://example.com/htsnexus_test_NA12878.bam")
            .with_headers(Headers::default().with_header("Range", "bytes=2596771-2596798"))
            .with_class(Body),
        ],
      ));
      assert_eq!(response, expected_response);

      // let mut headers = HeaderMap::default();
      // let options = test_range_options(&mut headers);
      //
      // assert_eq!(
      //   storage.range_url("assets/key1", options).await.unwrap(),
      //   HtsGetUrl::new(format!("{}/assets/key1", url))
      //     .with_headers(Headers::default().with_header(AUTHORIZATION.as_str(), "secret"))
      // );
    })
      .await;
  }


  fn test_client() -> Client<HttpsConnector<HttpConnector>> {
    Client::builder().build(
      HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build(),
    )
  }

  pub(crate) async fn with_url_test_server<F, Fut>(test: F)
  where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (_, base_path) = create_local_test_files().await;
    with_test_server(base_path.path(), test).await;
  }

  async fn test_auth<B>(
    request: Request<B>,
    next: Next<B>,
  ) -> result::Result<Response, StatusCode> {
    let auth_header = request
      .headers()
      .get(AUTHORIZATION)
      .and_then(|header| header.to_str().ok());

    match auth_header {
      Some("secret") => Ok(next.run(request).await),
      _ => Err(StatusCode::UNAUTHORIZED),
    }
  }

  pub(crate) async fn with_test_server<F, Fut>(server_base_path: &Path, test: F)
  where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = ()>,
  {
    let mut router = Router::new()
      .route("/endpoint_file/:id", get(|| async {
        let mut bytes = vec![];
        File::open("data/bam/htsnexus_test_NA12878.bam").await.unwrap().read_to_end(&mut bytes).await.unwrap();

        let bytes = bytes[..4668].to_vec();

        let stream = ReaderStream::new(Cursor::new(bytes));
        let body = StreamBody::new(stream);

        (StatusCode::OK, body).into_response()
      }))
      .route("/endpoint_index/:id", get(|| async {
        let mut bytes = vec![];
        File::open("data/bam/htsnexus_test_NA12878.bam.bai").await.unwrap().read_to_end(&mut bytes).await.unwrap();

        let stream = ReaderStream::new(Cursor::new(bytes));
        let body = StreamBody::new(stream);

        (StatusCode::OK, body).into_response()
      }))
      .route("/endpoint_head/:id", head(|| async {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Length", HeaderValue::from_static("2596799"));

        (StatusCode::OK, headers).into_response()
      }))
      .nest_service("/assets", ServeDir::new(server_base_path.to_str().unwrap()))
      .route_layer(middleware::from_fn(test_auth));

    #[cfg(feature = "crypt4gh")]
    {
      router = router.route("/endpoint_crypt4gh_header/:id", head(|| async {
        let length: u64 = 124;
        let bytes = length.to_le_bytes().to_vec();

        let stream = ReaderStream::new(Cursor::new(bytes));
        let body = StreamBody::new(stream);

        (StatusCode::OK, body).into_response()
      }));
    }

    // TODO fix this in htsget-test to bind and return tcp listener.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(
      axum::Server::from_tcp(listener)
        .unwrap()
        .serve(router.into_make_service()),
    );

    test(format!("http://{}", addr)).await;
  }

  fn test_headers(headers: &mut HeaderMap) -> &HeaderMap {
    headers.append(
      HeaderName::from_str(AUTHORIZATION.as_str()).unwrap(),
      HeaderValue::from_str("secret").unwrap(),
    );
    headers
  }

  fn test_range_options(headers: &mut HeaderMap) -> RangeUrlOptions {
    let headers = test_headers(headers);
    let options = RangeUrlOptions::new_with_default_range(headers);

    options
  }
}
