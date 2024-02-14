#[cfg(feature = "crypt4gh")]
pub mod encrypt;

use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::MapErr;
use futures_util::TryStreamExt;
use http::header::CONTENT_LENGTH;
use http::{HeaderMap, Method, Request, Response, Uri};
use hyper::client::HttpConnector;
use hyper::{Body, Client, Error};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use pin_project::pin_project;
use tokio::io::{AsyncRead, ReadBuf};
use tokio_util::io::StreamReader;
use tracing::{debug, instrument};
#[cfg(feature = "crypt4gh")]
use {
  crate::storage::{BytesPosition, BytesRange},
  async_crypt4gh::edit_lists::{ClampedPosition, UnencryptedPosition},
  async_crypt4gh::reader::builder::Builder,
  async_crypt4gh::reader::Reader,
  async_crypt4gh::PublicKey,
  base64::engine::general_purpose,
  base64::Engine,
  crypt4gh::Keys,
  htsget_config::types::Class,
  http::header::InvalidHeaderValue,
  http::header::RANGE,
  mockall_double::double,
  tokio_rustls::rustls::PrivateKey,
};

#[cfg(feature = "crypt4gh")]
#[double]
use crate::storage::url::encrypt::Encrypt;
use crate::storage::StorageError::{InternalError, KeyNotFound, ResponseError, UrlParseError};
use crate::storage::{
  BytesPositionOptions, DataBlock, GetOptions, HeadOptions, HeadOutput, RangeUrlOptions, Result,
  Storage, StorageError,
};
use crate::Url as HtsGetUrl;
use htsget_config::error;
use htsget_config::storage::url::endpoints::Endpoints;
use htsget_config::types::KeyType;

pub const CLIENT_PUBLIC_KEY_NAME: &str = "client-public-key";
pub const SERVER_PUBLIC_KEY_NAME: &str = "server-public-key";
pub const CLIENT_ADDITIONAL_BYTES: &str = "client-additional-bytes";
pub const SERVER_ADDITIONAL_BYTES: &str = "server-additional-bytes";

/// A storage struct which derives data from HTTP URLs.
#[derive(Debug, Clone)]
pub struct UrlStorage {
  client: Client<HttpsConnector<HttpConnector>>,
  endpoints: Endpoints,
  response_url: Uri,
  forward_headers: bool,
  #[cfg(feature = "crypt4gh")]
  encrypt: Encrypt,
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
      #[cfg(feature = "crypt4gh")]
      encrypt: Default::default(),
    }
  }

  #[cfg(feature = "crypt4gh")]
  /// Add the key generator.
  pub fn with_key_gen(mut self, key_gen: Encrypt) -> Self {
    self.encrypt = key_gen;
    self
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
      #[cfg(feature = "crypt4gh")]
      encrypt: Default::default(),
    }
  }

  /// Construct the Crypt4GH query.
  #[cfg(feature = "crypt4gh")]
  fn encode_key(public_key: &PublicKey) -> String {
    general_purpose::STANDARD.encode(public_key.get_ref())
  }

  /// Decode a public key using base64.
  #[cfg(feature = "crypt4gh")]
  fn decode_public_key(headers: &HeaderMap, name: &str) -> Result<Vec<u8>> {
    general_purpose::STANDARD
      .decode(
        headers
          .get(name)
          .ok_or_else(|| StorageError::InvalidInput("no public key found in header".to_string()))?
          .as_bytes(),
      )
      .map_err(|err| StorageError::InvalidInput(format!("failed to decode public key: {}", err)))
  }

  /// Get a url from the key.
  pub fn get_url_from_key<K: AsRef<str> + Send>(&self, key: K, endpoint: &Uri) -> Result<Uri> {
    // Todo: proper url parsing here, probably with the `url` crate.
    let uri = if endpoint.to_string().ends_with('/') {
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
      .send_request(key, headers, Method::HEAD, self.endpoints.file())
      .await
  }

  /// Get the key.
  pub async fn get_header<K: AsRef<str> + Send>(
    &self,
    key: K,
    headers: &HeaderMap,
  ) -> Result<Response<Body>> {
    self
      .send_request(key, headers, Method::GET, self.endpoints.file())
      .await
  }

  /// Get the key.
  pub async fn get_index<K: AsRef<str> + Send>(
    &self,
    key: K,
    headers: &HeaderMap,
  ) -> Result<Response<Body>> {
    self
      .send_request(key, headers, Method::GET, self.endpoints.index())
      .await
  }
}

/// Type representing the `StreamReader` for `UrlStorage`.
pub type UrlStreamReader = StreamReader<MapErr<Body, fn(Error) -> StorageError>, Bytes>;

/// An enum representing the variants of a stream reader. Note, cannot use tokio_util::Either
/// directly because this needs to be gated behind a feature flag.
/// Todo, make this less ugly, better separate feature flags.
#[pin_project(project = ProjectUrlStream)]
pub enum UrlStreamEither {
  A(#[pin] UrlStreamReader),
  #[cfg(feature = "crypt4gh")]
  B(#[pin] Crypt4GHReader),
}

#[cfg(feature = "crypt4gh")]
#[pin_project]
pub struct Crypt4GHReader {
  #[pin]
  reader: Reader<UrlStreamReader>,
  client_additional_bytes: Option<u64>,
}

#[cfg(feature = "crypt4gh")]
impl AsyncRead for Crypt4GHReader {
  fn poll_read(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<std::io::Result<()>> {
    self.project().reader.poll_read(cx, buf)
  }
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
    _head_output: Option<HeadOutput>,
  ) -> Result<Self::Streamable> {
    let key = key.as_ref().to_string();
    debug!(calling_from = ?self, key, "getting file with key {:?}", key);

    match KeyType::from_ending(&key) {
      KeyType::File => {
        #[cfg(feature = "crypt4gh")]
        if options.object_type.is_crypt4gh() {
          let key_pair = if let Some(key_pair) = options.object_type.crypt4gh_key_pair() {
            key_pair.key_pair().clone()
          } else {
            self
              .encrypt
              .generate_key_pair()
              .map_err(|err| UrlParseError(err.to_string()))?
          };

          let mut headers = options.request_headers().clone();
          headers.append(
            SERVER_PUBLIC_KEY_NAME,
            Self::encode_key(key_pair.public_key())
              .try_into()
              .map_err(|err: InvalidHeaderValue| UrlParseError(err.to_string()))?,
          );

          // Additional length for the header.
          let output_headers = _head_output
            .as_ref()
            .and_then(|output| output.response_headers());

          let additional_header_length: Option<u64> = output_headers
            .and_then(|headers| headers.get(SERVER_ADDITIONAL_BYTES))
            .and_then(|length| length.to_str().ok())
            .and_then(|length| length.parse().ok());

          let file_size: Option<u64> = output_headers
            .and_then(|headers| headers.get(CONTENT_LENGTH))
            .and_then(|length| length.to_str().ok())
            .and_then(|length| length.parse().ok());

          if let (Some(crypt4gh_header_length), Some(file_size)) =
            (additional_header_length, file_size)
          {
            let range = options.range;
            let range = range.convert_to_crypt4gh_ranges(crypt4gh_header_length, file_size);

            if let Some(end) = range.get_end() {
              let range = range.with_end(end + additional_header_length.unwrap_or_default());

              let range: String = String::from(&BytesRange::from(&range));
              if !range.is_empty() {
                headers.append(
                  RANGE,
                  range
                    .parse()
                    .map_err(|err: InvalidHeaderValue| UrlParseError(err.to_string()))?,
                );
              }
            }
          }

          let response = self.get_header(key.to_string(), &headers).await?;

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

          // Additional length for the header.
          let client_additional_bytes: Option<u64> = output_headers
            .and_then(|headers| {
              headers
                .get(CLIENT_ADDITIONAL_BYTES)
                .or_else(|| headers.get(SERVER_ADDITIONAL_BYTES))
            })
            .and_then(|length| length.to_str().ok())
            .and_then(|length| length.parse().ok());

          return Ok(UrlStreamEither::B(Crypt4GHReader {
            reader,
            client_additional_bytes,
          }));
        }

        Ok(
          self
            .get_header(key.to_string(), options.request_headers())
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
  ) -> Result<HeadOutput> {
    let key = key.as_ref();
    let head = self.head_key(key, options.request_headers()).await?;

    let len: u64 = head
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
    Ok(HeadOutput::new(len, Some(head.headers().clone())))
  }

  #[instrument(level = "trace", skip(self, reader))]
  async fn update_byte_positions(
    &self,
    reader: Self::Streamable,
    positions_options: BytesPositionOptions<'_>,
  ) -> Result<Vec<DataBlock>> {
    match reader {
      #[cfg(feature = "crypt4gh")]
      UrlStreamEither::B(reader) if positions_options.object_type.is_crypt4gh() => {
        let Crypt4GHReader {
          reader,
          client_additional_bytes,
        } = reader;

        let keys = reader
          .keys()
          .first()
          .ok_or_else(|| UrlParseError("missing crypt4gh keys from reader".to_string()))?;
        let file_size = positions_options.file_size();

        let client_additional_bytes = if let Some(bytes) = client_additional_bytes {
          bytes
        } else {
          reader
            .header_size()
            .ok_or_else(|| UrlParseError("crypt4gh header has not been read".to_string()))?
        };

        let recipient_public_key =
          Self::decode_public_key(positions_options.headers, CLIENT_PUBLIC_KEY_NAME)?;

        let unencrypted_positions = BytesPosition::merge_all(positions_options.positions.clone());
        let clamped_positions = BytesPosition::merge_all(
          positions_options
            .positions
            .clone()
            .into_iter()
            .map(|pos| pos.convert_to_clamped_crypt4gh_ranges(file_size))
            .collect::<Vec<_>>(),
        );

        // Calculate edit lists
        let (header_info, edit_list_packet) = self.encrypt.edit_list(
          &reader,
          unencrypted_positions
            .iter()
            .map(|position| {
              UnencryptedPosition::new(
                position.start.unwrap_or_default(),
                position.end.unwrap_or(file_size),
              )
            })
            .collect(),
          clamped_positions
            .iter()
            .map(|position| {
              ClampedPosition::new(
                position.start.unwrap_or_default(),
                position.end.unwrap_or(file_size),
              )
            })
            .collect(),
          PrivateKey(keys.privkey.clone()),
          PublicKey::new(recipient_public_key),
        )?;

        let encrypted_positions = BytesPosition::merge_all(
          positions_options
            .positions
            .clone()
            .into_iter()
            .map(|pos| pos.convert_to_crypt4gh_ranges(client_additional_bytes, file_size))
            .collect::<Vec<_>>(),
        );

        // Append header with edit lists attached.
        let header_info_size = header_info.len() as u64;
        let mut blocks = vec![
          DataBlock::Data(header_info, Some(Class::Header)),
          DataBlock::Range(
            BytesPosition::default()
              .with_start(header_info_size)
              .with_end(client_additional_bytes),
          ),
          DataBlock::Data(edit_list_packet, Some(Class::Header)),
        ];
        blocks.extend(DataBlock::from_bytes_positions(encrypted_positions));

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
  #[cfg(feature = "crypt4gh")]
  use {
    async_crypt4gh::KeyPair,
    crypt4gh::encrypt,
    htsget_config::resolver::object::{ObjectType, TaggedObjectTypes},
    htsget_config::tls::crypt4gh::Crypt4GHKeyPair,
    htsget_test::crypt4gh::get_encryption_keys,
    htsget_test::http_tests::test_bam_crypt4gh_byte_ranges,
    std::collections::HashSet,
  };

  use htsget_config::types::Class::{Body, Header};
  use htsget_config::types::Request as HtsgetRequest;
  use htsget_config::types::{Format, Headers, Query, Url};
  use htsget_test::http_tests::default_dir;
  use htsget_test::http_tests::{parse_as_bgzf, test_bam_file_byte_ranges};

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
      endpoints_test(),
      Uri::from_str("https://localhost:8080").unwrap(),
      true,
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
        endpoints_from_url(&url),
        Uri::from_str(&url).unwrap(),
        true,
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
        endpoints_from_url(&url),
        Uri::from_str(&url).unwrap(),
        true,
      );

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);
      let object_type = Default::default();
      let options = GetOptions::new_with_default_range(headers, &object_type);

      let mut reader = storage.get("assets/key1", options, None).await.unwrap();

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

      assert_eq!(
        storage
          .head("assets/key1", options)
          .await
          .unwrap()
          .content_length(),
        6
      );
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

      let expected_response = Ok(expected_bam_response());
      assert_eq!(response, expected_response);

      let (bytes, _) = test_bam_file_byte_ranges(
        response.unwrap(),
        default_dir().join("data/bam/htsnexus_test_NA12878.bam"),
      )
      .await;

      parse_as_bgzf(bytes).await;
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

      let mut key_gen = Encrypt::default();
      key_gen
        .expect_generate_key_pair()
        .times(1)
        .returning(|| Ok(expected_key_pair()));
      key_gen
        .expect_edit_list()
        .times(1)
        .returning(|_, _, _, _, _| Ok(expected_edit_list()));
      let storage = storage.with_key_gen(key_gen);

      let (_, public_key) = get_encryption_keys().await;
      let mut header_map = HeaderMap::default();
      let public_key = general_purpose::STANDARD.encode(public_key);
      test_headers(&mut header_map);
      header_map.append(
        HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
        HeaderValue::from_str(&public_key).unwrap(),
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

      assert_encrypted_endpoints(&public_key, response).await;
    })
    .await;
  }

  #[cfg(feature = "crypt4gh")]
  #[tokio::test]
  async fn test_endpoints_with_predefined_key_pair() {
    with_url_test_server(|url| async move {
      let storage = UrlStorage::new(
        test_client(),
        endpoints_from_url_with_path(&url),
        Uri::from_str("http://example.com").unwrap(),
        true,
      );

      let mut key_gen = Encrypt::default();
      key_gen
        .expect_edit_list()
        .times(1)
        .returning(|_, _, _, _, _| Ok(expected_edit_list()));
      let storage = storage.with_key_gen(key_gen);

      let (_, public_key) = get_encryption_keys().await;
      let mut header_map = HeaderMap::default();
      let public_key = general_purpose::STANDARD.encode(public_key);
      test_headers(&mut header_map);
      header_map.append(
        HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
        HeaderValue::from_str(&public_key).unwrap(),
      );

      let request =
        HtsgetRequest::new_with_id("htsnexus_test_NA12878".to_string()).with_headers(header_map);
      let query = Query::new(
        "htsnexus_test_NA12878",
        Format::Bam,
        request,
        ObjectType::Crypt4GH {
          crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        },
      )
      .with_reference_name("11")
      .with_start(5015000)
      .with_end(5050000);

      let searcher = HtsGetFromStorage::new(storage);
      let response = searcher.search(query.clone()).await.unwrap();

      assert_encrypted_endpoints(&public_key, response).await;
    })
    .await;
  }

  #[cfg(feature = "crypt4gh")]
  fn expected_key_pair() -> KeyPair {
    KeyPair::new(
      PrivateKey(vec![
        162, 124, 25, 18, 207, 218, 241, 41, 162, 107, 29, 40, 10, 93, 30, 193, 104, 42, 176, 235,
        207, 248, 126, 230, 97, 205, 253, 224, 215, 160, 67, 239,
      ]),
      PublicKey::new(vec![
        56, 44, 122, 180, 24, 116, 207, 149, 165, 49, 204, 77, 224, 136, 232, 121, 209, 249, 23,
        51, 120, 2, 187, 147, 82, 227, 232, 32, 17, 223, 7, 38,
      ]),
    )
  }

  #[cfg(feature = "crypt4gh")]
  fn expected_edit_list() -> (Vec<u8>, Vec<u8>) {
    (
      vec![99, 114, 121, 112, 116, 52, 103, 104, 1, 0, 0, 0, 2, 0, 0, 0],
      vec![
        124, 0, 0, 0, 0, 0, 0, 0, 56, 44, 122, 180, 24, 116, 207, 149, 165, 49, 204, 77, 224, 136,
        232, 121, 209, 249, 23, 51, 120, 2, 187, 147, 82, 227, 232, 32, 17, 223, 7, 38, 10, 170,
        72, 177, 188, 32, 68, 101, 239, 249, 47, 182, 51, 120, 65, 239, 9, 93, 149, 225, 207, 244,
        103, 224, 99, 35, 94, 187, 25, 202, 122, 49, 52, 40, 131, 144, 19, 142, 223, 245, 152, 170,
        3, 70, 0, 146, 64, 18, 159, 109, 26, 245, 246, 169, 59, 232, 6, 210, 128, 183, 93, 77, 199,
        138, 203, 200, 156, 50, 114, 159, 109, 130, 128, 208, 179, 41, 67, 161, 57, 78, 0, 68, 39,
        103,
      ],
    )
  }

  #[cfg(feature = "crypt4gh")]
  async fn assert_encrypted_endpoints(public_key: &String, response: HtsgetResponse) {
    let expected_response = HtsgetResponse::new(
      Format::Bam,
      vec![
        // header info
        Url::new("data:;base64,Y3J5cHQ0Z2gBAAAAAgAAAA=="),
        // original header
        Url::new("http://example.com/htsnexus_test_NA12878.bam.c4gh").with_headers(
          Headers::default()
            .with_header("authorization", "secret")
            .with_header(CLIENT_PUBLIC_KEY_NAME, public_key)
            .with_header("Range", format!("bytes={}-{}", 16, 123)),
        ),
        // edit list packet
        Url::new(
          "data:;base64,fAAAAAAAAAA4LHq0GHTPlaUxzE3giOh50fkXM3gCu5NS4+ggEd8HJgqqSLG8IERl7/kvt\
            jN4Qe8JXZXhz/Rn4GMjXrsZynoxNCiDkBOO3/WYqgNGAJJAEp9tGvX2qTvoBtKAt11Nx4rLyJwycp9tgoDQsylD\
            oTlOAEQnZw==",
        ),
        Url::new("http://example.com/htsnexus_test_NA12878.bam.c4gh").with_headers(
          Headers::default()
            .with_header("authorization", "secret")
            .with_header(CLIENT_PUBLIC_KEY_NAME, public_key)
            .with_header("Range", format!("bytes={}-{}", 124, 124 + 65564 - 1)),
        ),
        Url::new("http://example.com/htsnexus_test_NA12878.bam.c4gh").with_headers(
          Headers::default()
            .with_header("authorization", "secret")
            .with_header(CLIENT_PUBLIC_KEY_NAME, public_key)
            .with_header(
              "Range",
              format!("bytes={}-{}", 124 + 196692, 124 + 1114588 - 1),
            ),
        ),
        Url::new("http://example.com/htsnexus_test_NA12878.bam.c4gh").with_headers(
          Headers::default()
            .with_header("authorization", "secret")
            .with_header(CLIENT_PUBLIC_KEY_NAME, public_key)
            .with_header("Range", format!("bytes={}-{}", 124 + 2556996, 2598043 - 1)),
        ),
      ],
    );

    assert_eq!(response, expected_response);

    let (bytes, _) = test_bam_file_byte_ranges(
      response,
      default_dir().join("data/crypt4gh/htsnexus_test_NA12878.bam.c4gh"),
    )
    .await;

    let (expected_bytes, _) = test_bam_file_byte_ranges(
      expected_bam_response(),
      default_dir().join("data/bam/htsnexus_test_NA12878.bam"),
    )
    .await;

    test_bam_crypt4gh_byte_ranges(bytes.clone(), expected_bytes).await;
  }

  fn expected_bam_response() -> HtsgetResponse {
    HtsgetResponse::new(
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
    )
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
    let router = Router::new()
      .route(
        "/endpoint_file/:id",
        get(|_headers: HeaderMap| async move {
          #[cfg(feature = "crypt4gh")]
          if _headers.contains_key(SERVER_PUBLIC_KEY_NAME) {
            let range = _headers.get(RANGE).unwrap().to_str().unwrap();
            let range = BytesPosition::from_str(range).unwrap();

            let mut bytes = vec![];
            let path = default_dir().join("data/bam/htsnexus_test_NA12878.bam");
            File::open(path)
              .await
              .unwrap()
              .read_to_end(&mut bytes)
              .await
              .unwrap();

            let encryption_keys = KeyPair::new(
              PrivateKey(vec![
                161, 61, 174, 214, 146, 101, 139, 42, 247, 73, 68, 96, 8, 198, 29, 26, 68, 113,
                200, 182, 20, 217, 151, 89, 211, 14, 110, 80, 111, 138, 255, 194,
              ]),
              PublicKey::new(vec![
                249, 209, 232, 54, 131, 32, 40, 191, 15, 205, 151, 70, 90, 37, 149, 101, 55, 138,
                22, 59, 176, 0, 59, 7, 167, 10, 194, 129, 55, 147, 141, 101,
              ]),
            );

            let keys = Keys {
              method: 0,
              privkey: encryption_keys.private_key().clone().0,
              recipient_pubkey: general_purpose::STANDARD
                .decode(_headers.get(SERVER_PUBLIC_KEY_NAME).unwrap())
                .unwrap(),
            };

            let mut read_buf = Cursor::new(bytes);
            let mut write_buf = Cursor::new(vec![]);

            encrypt(
              &HashSet::from_iter(vec![keys]),
              &mut read_buf,
              &mut write_buf,
              0,
              None,
            )
            .unwrap();

            let data = write_buf.into_inner();

            let data = match (range.start, range.end) {
              (None, None) => data,
              (Some(start), None) => data[start as usize..].to_vec(),
              (None, Some(end)) => data[..end as usize].to_vec(),
              (Some(start), Some(end)) => data[start as usize..end as usize].to_vec(),
            };

            let stream = ReaderStream::new(Cursor::new(data));
            let body = StreamBody::new(stream);

            return (StatusCode::OK, body).into_response();
          }

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
        "/endpoint_file/:id",
        head(|AxumPath(id): AxumPath<String>| async move {
          let length = if id == "htsnexus_test_NA12878.bam.c4gh" {
            "2598043"
          } else {
            "2596799"
          };

          Response::builder()
            .header(SERVER_ADDITIONAL_BYTES, 100)
            .header(CLIENT_ADDITIONAL_BYTES, 124)
            .header(CONTENT_LENGTH, HeaderValue::from_static(length))
            .status(StatusCode::OK)
            .body(StreamBody::new(ReaderStream::new(Cursor::new(vec![]))))
            .unwrap()
            .into_response()
        }),
      )
      .nest_service("/assets", ServeDir::new(server_base_path.to_str().unwrap()))
      .route_layer(middleware::from_fn(test_auth));

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
    )
  }

  fn endpoints_from_url(url: &str) -> Endpoints {
    Endpoints::new(
      Uri::from_str(url).unwrap().into(),
      Uri::from_str(url).unwrap().into(),
    )
  }

  fn endpoints_from_url_with_path(url: &str) -> Endpoints {
    Endpoints::new(
      Uri::from_str(&format!("{}/endpoint_index", url))
        .unwrap()
        .into(),
      Uri::from_str(&format!("{}/endpoint_file", url))
        .unwrap()
        .into(),
    )
  }
}
