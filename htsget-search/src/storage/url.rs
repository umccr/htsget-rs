use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use base64::engine::general_purpose;
use base64::Engine;
use bytes::Bytes;
use crypt4gh::Keys;
use futures_util::stream::MapErr;
use futures_util::TryStreamExt;
use http::header::CONTENT_LENGTH;
use http::{HeaderMap, Method, Request, Response, Uri};
use hyper::client::HttpConnector;
use hyper::{Body, Client, Error};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use pin_project::pin_project;
use tokio::io::{AsyncRead, ReadBuf};
use tokio_rustls::rustls::PrivateKey;
use tokio_util::io::StreamReader;
use tracing::{debug, instrument};

use async_crypt4gh::edit_lists::{add_edit_list, UnencryptedPosition};
use async_crypt4gh::reader::builder::Builder;
use async_crypt4gh::reader::Reader;
use async_crypt4gh::util::generate_key_pair;
use async_crypt4gh::PublicKey;
use htsget_config::error;
use htsget_config::storage::url::endpoints::Endpoints;
use htsget_config::types::{Class, KeyType};

use crate::storage::StorageError::{InternalError, KeyNotFound, ResponseError, UrlParseError};
use crate::storage::{
  BytesPositionOptions, DataBlock, GetOptions, HeadOptions, RangeUrlOptions, Result, Storage,
  StorageError,
};
use crate::Url as HtsGetUrl;

const PUBLIC_KEY_NAME: &str = "publicKey";

/// A storage struct which derives data from HTTP URLs.
#[derive(Debug, Clone)]
pub struct UrlStorage {
  client: Client<HttpsConnector<HttpConnector>>,
  endpoints: Endpoints,
  response_url: Uri,
  forward_headers: bool,
}

impl UrlStorage {
  /// Construct a new UrlStorage.
  pub fn new(
    client: Client<HttpsConnector<HttpConnector>>,
    endpoints: Endpoints,
    response_url: Uri,
    forward_headers: bool,
  ) -> Self {
    Self {
      client,
      endpoints,
      response_url,
      forward_headers,
    }
  }

  /// Construct a new UrlStorage with a default client.
  pub fn new_with_default_client(
    endpoints: Endpoints,
    response_url: Uri,
    forward_headers: bool,
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
      endpoints,
      response_url,
      forward_headers,
    }
  }

  /// Construct the Crypt4GH query.
  #[cfg(feature = "crypt4gh")]
  fn crypt4gh_query(public_key: &PublicKey) -> String {
    format!(
      "?{PUBLIC_KEY_NAME}={}",
      general_purpose::STANDARD.encode(public_key.get_ref())
    )
  }

  /// Decode a public key using base64.
  #[cfg(feature = "crypt4gh")]
  fn decode_public_key(headers: &HeaderMap) -> Result<Vec<u8>> {
    general_purpose::STANDARD
      .decode(
        headers
          .get(PUBLIC_KEY_NAME)
          .ok_or_else(|| StorageError::InvalidInput("no public key found in header".to_string()))?
          .as_bytes(),
      )
      .map_err(|err| StorageError::InvalidInput(format!("failed to decode public key: {}", err)))
  }

  /// Get a url from the key.
  pub fn get_url_from_key<K: AsRef<str> + Send>(
    &self,
    key: K,
    endpoint: &Uri,
    query: &str,
  ) -> Result<Uri> {
    // Todo: proper url parsing here, probably with the `url` crate.
    let uri = if endpoint.to_string().ends_with('/') {
      format!("{}{}{}", endpoint, key.as_ref(), query)
    } else {
      format!("{}/{}{}", endpoint, key.as_ref(), query)
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
    query: &str,
  ) -> Result<Response<Body>> {
    let key = key.as_ref();
    let url = self.get_url_from_key(key, url, query)?;

    let request = Request::builder().method(method).uri(&url);

    let request = headers
      .iter()
      .fold(request, |acc, (key, value)| acc.header(key, value))
      .body(Body::empty())
      .map_err(|err| UrlParseError(err.to_string()))?;

    debug!("Calling with request: {:#?}", &request);

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
    let url = self.get_url_from_key(key, endpoint, "")?.into_parts();
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
      .send_request(key, headers, Method::HEAD, self.endpoints.head(), "")
      .await
  }

  /// Get the key.
  pub async fn get_header<K: AsRef<str> + Send>(
    &self,
    key: K,
    headers: &HeaderMap,
    query: String,
  ) -> Result<Response<Body>> {
    self
      .send_request(key, headers, Method::GET, self.endpoints.file(), &query)
      .await
  }

  /// Get the key.
  pub async fn get_index<K: AsRef<str> + Send>(
    &self,
    key: K,
    headers: &HeaderMap,
  ) -> Result<Response<Body>> {
    self
      .send_request(key, headers, Method::GET, self.endpoints.index(), "")
      .await
  }
}

/// Type representing the `StreamReader` for `UrlStorage`.
pub type UrlStreamReader = StreamReader<MapErr<Body, fn(Error) -> StorageError>, Bytes>;

/// An enum representing the variants of a stream reader. Note, cannot use tokio_util::Either
/// directly because this needs to be gated behind a feature flag.
/// Todo, rework this to look nicer and better separate feature flags.
#[pin_project(project = ProjectUrlStream)]
pub enum UrlStreamEither {
  A(#[pin] UrlStreamReader),
  #[cfg(feature = "crypt4gh")]
  B(#[pin] Reader<UrlStreamReader>),
}

impl AsyncRead for UrlStreamEither {
  fn poll_read(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<std::io::Result<()>> {
    match self.project() {
      ProjectUrlStream::A(a) => a.poll_read(cx, buf),
      #[cfg(feature = "crypt4gh")]
      ProjectUrlStream::B(b) => b.poll_read(cx, buf),
    }
  }
}

impl From<Response<Body>> for UrlStreamEither {
  fn from(response: Response<Body>) -> Self {
    Self::A(StreamReader::new(response.into_body().map_err(|err| {
      ResponseError(format!("reading body from response: {}", err))
    })))
  }
}

#[async_trait]
impl Storage for UrlStorage {
  type Streamable = UrlStreamEither;

  #[instrument(level = "trace", skip(self))]
  async fn get<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    options: GetOptions<'_>,
  ) -> Result<Self::Streamable> {
    let key = key.as_ref().to_string();
    debug!(calling_from = ?self, key, "getting file with key {:?}", key);

    match KeyType::from_ending(&key) {
      KeyType::File => {
        #[cfg(feature = "crypt4gh")]
        if options.object_type.is_crypt4gh() {
          let key_pair = generate_key_pair().map_err(|err| UrlParseError(err.to_string()))?;
          let query = Self::crypt4gh_query(key_pair.public_key());

          let response = self
            .get_header(key.to_string(), options.request_headers(), query)
            .await?;

          let crypt4gh_keys = Keys {
            method: 0,
            privkey: key_pair.private_key().clone().0,
            recipient_pubkey: key_pair.public_key().clone().into_inner(),
          };
          let stream_reader: UrlStreamReader = StreamReader::new(
            response
              .into_body()
              .map_err(|err| ResponseError(format!("reading body from response: {}", err))),
          );
          let reader = Builder::default().build_with_reader(stream_reader, vec![crypt4gh_keys]);

          return Ok(UrlStreamEither::B(reader));
        }

        Ok(
          self
            .get_header(key.to_string(), options.request_headers(), "".to_string())
            .await?
            .into(),
        )
      }
      KeyType::Index => Ok(
        self
          .get_index(key.to_string(), options.request_headers())
          .await?
          .into(),
      ),
    }
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

  #[instrument(level = "trace", skip(self, reader))]
  async fn update_byte_positions(
    &self,
    reader: Self::Streamable,
    mut positions_options: BytesPositionOptions<'_>,
  ) -> Result<Vec<DataBlock>> {
    match reader {
      #[cfg(feature = "crypt4gh")]
      UrlStreamEither::B(reader) if positions_options.object_type.is_crypt4gh() => {
        let keys = reader
          .keys()
          .first()
          .ok_or_else(|| UrlParseError("missing crypt4gh keys from reader".to_string()))?;
        let file_size = positions_options.file_size();

        let header_read_error = || UrlParseError("crypt4gh header has not been read".to_string());
        let header_size = reader.header_size().ok_or_else(header_read_error)?;

        let recipient_public_key = Self::decode_public_key(positions_options.headers)?;

        // Calculate edit lists
        let reencrypted_header = add_edit_list(
          &reader,
          positions_options
            .positions
            .iter()
            .map(|position| {
              UnencryptedPosition::new(
                position.start.unwrap_or_default(),
                position.end.unwrap_or(file_size),
              )
            })
            .collect(),
          PrivateKey(keys.privkey.clone()),
          PublicKey::new(recipient_public_key),
          file_size,
        )
        .await
        .map_err(|err| UrlParseError(err.to_string()))?
        .ok_or_else(header_read_error)?;

        // Note original header byte length.
        positions_options = positions_options.convert_to_crypt4gh_ranges(header_size, file_size);

        // Append header with edit lists attached.
        let mut blocks = vec![DataBlock::Data(reencrypted_header, Some(Class::Header))];
        blocks.extend(DataBlock::from_bytes_positions(
          positions_options.merge_all().into_inner(),
        ));

        Ok(blocks)
      }
      _ => Ok(DataBlock::from_bytes_positions(
        positions_options.merge_all().into_inner(),
      )),
    }
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

  use axum::body::StreamBody;
  use axum::extract::Path as AxumPath;
  use axum::middleware::Next;
  use axum::response::{IntoResponse, Response};
  use axum::routing::{get, head};
  use axum::{middleware, Router};
  use http::header::AUTHORIZATION;
  use http::{HeaderName, HeaderValue, Request, StatusCode};
  use hyper::body::to_bytes;
  use tokio::fs::File;
  use tokio::io::AsyncReadExt;
  use tokio_util::io::ReaderStream;
  use tower_http::services::ServeDir;

  use htsget_config::resolver::object::{ObjectType, TaggedObjectTypes};
  use htsget_config::types::Class::{Body, Header};
  use htsget_config::types::Request as HtsgetRequest;
  use htsget_config::types::{Format, Headers, Query, Url};
  use htsget_test::crypt4gh::get_encryption_keys;
  use htsget_test::http_tests::default_dir;
  use htsget_test::http_tests::test_bam_file_byte_ranges;

  use crate::htsget::from_storage::HtsGetFromStorage;
  use crate::htsget::HtsGet;
  use crate::storage::local::tests::create_local_test_files;
  use crate::Response as HtsgetResponse;

  use super::*;

  #[test]
  fn get_url_from_key() {
    let storage = UrlStorage::new(
      test_client(),
      endpoints_test(),
      Uri::from_str("https://localhost:8080").unwrap(),
      true,
    );

    assert_eq!(
      storage
        .get_url_from_key(
          "assets/key1",
          &Uri::from_str("https://example.com").unwrap(),
          ""
        )
        .unwrap(),
      Uri::from_str("https://example.com/assets/key1").unwrap()
    );
  }

  #[test]
  fn get_response_url_from_key() {
    let storage = UrlStorage::new(
      test_client(),
      endpoints_test(),
      Uri::from_str("https://localhost:8080").unwrap(),
      true,
    );

    assert_eq!(
      storage
        .get_url_from_key(
          "assets/key1",
          &Uri::from_str("https://localhost:8080").unwrap(),
          ""
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
        endpoints_from_url(&url),
        Uri::from_str(&url).unwrap(),
        true,
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
              "",
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
        endpoints_from_url(&url),
        Uri::from_str(&url).unwrap(),
        true,
      );

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);

      let response = String::from_utf8(
        to_bytes(
          storage
            .get_header("assets/key1", headers, "".to_string())
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
        endpoints_from_url(&url),
        Uri::from_str(&url).unwrap(),
        true,
      );

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);

      let response: u64 = storage
        .get_header("assets/key1", headers, "".to_string())
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
        endpoints_from_url(&url),
        Uri::from_str(&url).unwrap(),
        true,
      );

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);
      let object_type = Default::default();
      let options = GetOptions::new_with_default_range(headers, &object_type);

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
        endpoints_from_url(&url),
        Uri::from_str(&url).unwrap(),
        true,
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
        endpoints_from_url(&url),
        Uri::from_str(&url).unwrap(),
        true,
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
      endpoints_test(),
      Uri::from_str("https://localhost:8080").unwrap(),
      true,
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
      endpoints_test(),
      Uri::from_str("http://example.com").unwrap(),
      true,
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
      endpoints_test(),
      Uri::from_str("https://localhost:8081").unwrap(),
      false,
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
        endpoints_from_url_with_path(&url),
        Uri::from_str("http://example.com").unwrap(),
        true,
      );

      let mut header_map = HeaderMap::default();
      test_headers(&mut header_map);
      let request =
        HtsgetRequest::new_with_id("htsnexus_test_NA12878".to_string()).with_headers(header_map);
      let query = Query::new(
        "htsnexus_test_NA12878",
        Format::Bam,
        request,
        Default::default(),
      )
      .with_reference_name("11")
      .with_start(5015000)
      .with_end(5050000);

      let searcher = HtsGetFromStorage::new(storage);
      let response = searcher.search(query.clone()).await;

      let expected_response = Ok(HtsgetResponse::new(
        Format::Bam,
        vec![
          Url::new("http://example.com/htsnexus_test_NA12878.bam")
            .with_headers(
              Headers::default()
                .with_header("authorization", "secret")
                .with_header("Range", "bytes=0-4667"),
            )
            .with_class(Header),
          Url::new("http://example.com/htsnexus_test_NA12878.bam")
            .with_headers(
              Headers::default()
                .with_header("authorization", "secret")
                .with_header("Range", "bytes=256721-1065951"),
            )
            .with_class(Body),
          Url::new("http://example.com/htsnexus_test_NA12878.bam")
            .with_headers(
              Headers::default()
                .with_header("authorization", "secret")
                .with_header("Range", "bytes=2596771-2596798"),
            )
            .with_class(Body),
        ],
      ));
      assert_eq!(response, expected_response);

      test_bam_file_byte_ranges(
        response.unwrap(),
        default_dir().join("data/bam/htsnexus_test_NA12878.bam"),
      )
      .await
    })
    .await;
  }

  #[cfg(feature = "crypt4gh")]
  #[tokio::test]
  async fn test_endpoints_with_real_file_encrypted() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        test_client(),
        endpoints_from_url_with_path(&url),
        Uri::from_str("http://example.com").unwrap(),
        true,
      );

      let (_, public_key) = get_encryption_keys().await;
      let mut header_map = HeaderMap::default();
      test_headers(&mut header_map);
      header_map.append(
        HeaderName::from_str("publicKey").unwrap(),
        HeaderValue::from_str(&general_purpose::STANDARD.encode(public_key)).unwrap(),
      );

      let request =
        HtsgetRequest::new_with_id("htsnexus_test_NA12878".to_string()).with_headers(header_map);
      let query = Query::new(
        "htsnexus_test_NA12878",
        Format::Bam,
        request,
        ObjectType::Tagged(TaggedObjectTypes::GenerateKeys),
      )
      .with_reference_name("11")
      .with_start(5015000)
      .with_end(5050000);

      let searcher = HtsGetFromStorage::new(storage);
      let response = searcher.search(query.clone()).await.unwrap();

      let expected_response = HtsgetResponse::new(
        Format::Bam,
        vec![
          Url::new("http://example.com/htsnexus_test_NA12878.bam").with_headers(
            Headers::default()
              .with_header("authorization", "secret")
              .with_header("Range", format!("bytes=0-{}", 124 + 65564 - 1)),
          ),
          Url::new("http://example.com/htsnexus_test_NA12878.bam").with_headers(
            Headers::default()
              .with_header("authorization", "secret")
              .with_header(
                "Range",
                format!("bytes={}-{}", 124 + 196692, 124 + 1114588 - 1),
              ),
          ),
          Url::new("http://example.com/htsnexus_test_NA12878.bam").with_headers(
            Headers::default()
              .with_header("authorization", "secret")
              .with_header("Range", format!("bytes={}-{}", 124 + 2556996, 2598043 - 1)),
          ),
        ],
      );

      assert_eq!(response, expected_response);

      test_bam_file_byte_ranges(
        response,
        default_dir().join("data/crypt4gh/htsnexus_test_NA12878.bam.c4gh"),
      )
      .await
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
      .route(
        "/endpoint_file/:id",
        get(|| async {
          let mut bytes = vec![];
          let path = default_dir().join("data/bam/htsnexus_test_NA12878.bam");
          File::open(path)
            .await
            .unwrap()
            .read_to_end(&mut bytes)
            .await
            .unwrap();

          let bytes = bytes[..4668].to_vec();

          let stream = ReaderStream::new(Cursor::new(bytes));
          let body = StreamBody::new(stream);

          (StatusCode::OK, body).into_response()
        }),
      )
      .route(
        "/endpoint_index/:id",
        get(|AxumPath(id): AxumPath<String>| async move {
          if id == "htsnexus_test_NA12878.bam.bai" {
            let mut bytes = vec![];
            let path = default_dir().join("data/bam/htsnexus_test_NA12878.bam.bai");
            File::open(path)
              .await
              .unwrap()
              .read_to_end(&mut bytes)
              .await
              .unwrap();

            let stream = ReaderStream::new(Cursor::new(bytes));
            let body = StreamBody::new(stream);

            (StatusCode::OK, body).into_response()
          } else {
            let bytes: Vec<u8> = vec![];
            let stream = ReaderStream::new(Cursor::new(bytes));
            let body = StreamBody::new(stream);

            (StatusCode::NOT_FOUND, body).into_response()
          }
        }),
      )
      .route(
        "/endpoint_head/:id",
        head(|| async {
          let mut headers = HeaderMap::new();
          headers.insert("Content-Length", HeaderValue::from_static("2596799"));

          (StatusCode::OK, headers).into_response()
        }),
      )
      .nest_service("/assets", ServeDir::new(server_base_path.to_str().unwrap()))
      .route_layer(middleware::from_fn(test_auth));

    #[cfg(feature = "crypt4gh")]
    {
      router = router.route(
        "/endpoint_crypt4gh_header/:id",
        get(|| async {
          let length: u64 = 124;
          let bytes = length.to_le_bytes().to_vec();

          let stream = ReaderStream::new(Cursor::new(bytes));
          let body = StreamBody::new(stream);

          (StatusCode::OK, body).into_response()
        }),
      );
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

  fn endpoints_test() -> Endpoints {
    Endpoints::new(
      Uri::from_str("https://example.com").unwrap().into(),
      Uri::from_str("https://example.com").unwrap().into(),
      Uri::from_str("https://example.com").unwrap().into(),
    )
  }

  fn endpoints_from_url(url: &str) -> Endpoints {
    Endpoints::new(
      Uri::from_str(url).unwrap().into(),
      Uri::from_str(url).unwrap().into(),
      Uri::from_str(url).unwrap().into(),
    )
  }

  fn endpoints_from_url_with_path(url: &str) -> Endpoints {
    Endpoints::new(
      Uri::from_str(&format!("{}/endpoint_head", url))
        .unwrap()
        .into(),
      Uri::from_str(&format!("{}/endpoint_index", url))
        .unwrap()
        .into(),
      Uri::from_str(&format!("{}/endpoint_file", url))
        .unwrap()
        .into(),
    )
  }
}
