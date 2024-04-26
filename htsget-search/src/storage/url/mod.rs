#[cfg(feature = "crypt4gh")]
pub mod encrypt;

use std::fmt::Debug;
use std::pin::Pin;
use std::result;
use std::task::{Context, Poll};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::MapErr;
use futures_util::{Stream, TryStreamExt};
use http::header::{Entry, IntoHeaderName, CONTENT_LENGTH, USER_AGENT};
use http::{HeaderMap, Method, Request, Uri};
use hyper::Body;
use pin_project::pin_project;
use reqwest::{Client, ClientBuilder};
use tokio::io::{AsyncRead, ReadBuf};
use tokio_util::io::StreamReader;
use tracing::{debug, info, instrument};
#[cfg(feature = "crypt4gh")]
use {
  crate::storage::{BytesPosition, BytesRange},
  async_crypt4gh::edit_lists::{ClampedPosition, UnencryptedPosition},
  async_crypt4gh::reader::builder::Builder,
  async_crypt4gh::reader::Reader,
  async_crypt4gh::util::encode_public_key,
  async_crypt4gh::util::to_encrypted_file_size,
  async_crypt4gh::util::{read_public_key, to_unencrypted_file_size},
  async_crypt4gh::KeyPair,
  async_crypt4gh::PublicKey,
  base64::engine::general_purpose,
  base64::Engine,
  crypt4gh::Keys,
  htsget_config::types::Class,
  http::header::InvalidHeaderValue,
  http::header::RANGE,
  http::HeaderValue,
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
#[cfg(feature = "crypt4gh")]
use htsget_config::resolver::object::ObjectType;
use htsget_config::storage::url::endpoints::Endpoints;
use htsget_config::types::{KeyType, Query};

pub const CLIENT_PUBLIC_KEY_NAME: &str = "client-public-key";
pub const CLIENT_ADDITIONAL_BYTES: &str = "client-additional-bytes";
pub const SERVER_ADDITIONAL_BYTES: &str = "server-additional-bytes";

static HTSGET_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

/// A storage struct which derives data from HTTP URLs.
#[derive(Debug, Clone)]
pub struct UrlStorage {
  client: Client,
  endpoints: Endpoints,
  response_url: Uri,
  forward_headers: bool,
  user_agent: Option<String>,
  #[cfg(feature = "crypt4gh")]
  key_pair: Option<KeyPair>,
  #[cfg(feature = "crypt4gh")]
  encrypt: Encrypt,
}

impl UrlStorage {
  /// Construct a new UrlStorage.
  pub fn new(
    client: Client,
    endpoints: Endpoints,
    response_url: Uri,
    forward_headers: bool,
    user_agent: Option<String>,
    _query: &Query,
    #[cfg(feature = "crypt4gh")] _encrypt: Encrypt,
  ) -> Result<Self> {
    #[cfg(feature = "crypt4gh")]
    let mut key_pair = None;
    #[cfg(feature = "crypt4gh")]
    if _query.object_type().crypt4gh_key_pair().is_none() {
      key_pair = Some(
        _encrypt
          .generate_key_pair()
          .map_err(|err| UrlParseError(err.to_string()))?,
      );
    }

    Ok(Self {
      client,
      endpoints,
      response_url,
      forward_headers,
      user_agent,
      #[cfg(feature = "crypt4gh")]
      key_pair,
      #[cfg(feature = "crypt4gh")]
      encrypt: _encrypt,
    })
  }

  /// Construct a new UrlStorage with a default client.
  pub fn new_with_default_client(
    endpoints: Endpoints,
    response_url: Uri,
    forward_headers: bool,
    user_agent: Option<String>,
    _query: &Query,
    #[cfg(feature = "crypt4gh")] _encrypt: Encrypt,
  ) -> Result<Self> {
    #[cfg(feature = "crypt4gh")]
    let mut key_pair = None;
    #[cfg(feature = "crypt4gh")]
    if _query.object_type().crypt4gh_key_pair().is_none() {
      key_pair = Some(
        _encrypt
          .generate_key_pair()
          .map_err(|err| UrlParseError(err.to_string()))?,
      );
    }

    Ok(Self {
      client: ClientBuilder::new()
        .build()
        .map_err(|err| InternalError(format!("failed to build reqwest client: {}", err)))?,
      endpoints,
      response_url,
      forward_headers,
      user_agent,
      #[cfg(feature = "crypt4gh")]
      key_pair,
      #[cfg(feature = "crypt4gh")]
      encrypt: _encrypt,
    })
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
  ) -> Result<reqwest::Response> {
    let key = key.as_ref();
    let url = self.get_url_from_key(key, url)?;

    let request = Request::builder().method(method).uri(&url);

    let request = headers
      .iter()
      .fold(request, |acc, (key, value)| acc.header(key, value))
      .header(
        USER_AGENT,
        self.user_agent.as_deref().unwrap_or(HTSGET_USER_AGENT),
      )
      .body(Body::empty())
      .map_err(|err| UrlParseError(err.to_string()))?;

    debug!("Calling with request: {:#?}", &request);

    let response = self
      .client
      .execute(
        request
          .try_into()
          .map_err(|err| InternalError(format!("failed to create reqwest: {}", err)))?,
      )
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

    #[cfg(feature = "crypt4gh")]
    let key = if options
      .object_type()
      .send_encrypted_to_client()
      .is_some_and(|value| !value)
    {
      key.strip_suffix(".c4gh").unwrap_or(key)
    } else {
      key
    };

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
  ) -> Result<reqwest::Response> {
    self
      .send_request(key, headers, Method::HEAD, self.endpoints.file())
      .await
  }

  /// Get the key.
  pub async fn get_header<K: AsRef<str> + Send>(
    &self,
    key: K,
    headers: &HeaderMap,
  ) -> Result<reqwest::Response> {
    self
      .send_request(key, headers, Method::GET, self.endpoints.file())
      .await
  }

  /// Get the key.
  pub async fn get_index<K: AsRef<str> + Send>(
    &self,
    key: K,
    headers: &HeaderMap,
  ) -> Result<reqwest::Response> {
    self
      .send_request(key, headers, Method::GET, self.endpoints.index())
      .await
  }

  /// Remove all header entries from the header map.
  pub fn remove_header_entries<K: IntoHeaderName>(headers: &mut HeaderMap, key: K) {
    match headers.entry(key) {
      Entry::Occupied(entry) => {
        entry.remove_entry_mult();
      }
      Entry::Vacant(_) => {}
    }
  }

  /// Update the headers with the correct keys and user agent.
  #[cfg(feature = "crypt4gh")]
  pub async fn update_headers(
    &self,
    object_type: &ObjectType,
    headers: &HeaderMap,
  ) -> Result<(HeaderMap, KeyPair)> {
    let key_pair = if let Some(key_pair) = object_type.crypt4gh_key_pair() {
      let key_pair = key_pair.key_pair().clone();
      info!("Got key pair from config");
      key_pair
    } else {
      info!("Got key pair generated");
      self
        .key_pair
        .as_ref()
        .ok_or_else(|| InternalError("missing key pair".to_string()))?
        .clone()
    };

    let mut headers = headers.clone();
    Self::remove_header_entries(&mut headers, CLIENT_PUBLIC_KEY_NAME);
    Self::remove_header_entries(&mut headers, USER_AGENT);

    headers.append(
      CLIENT_PUBLIC_KEY_NAME,
      Self::encode_key(&PublicKey::new(
        encode_public_key(key_pair.public_key().clone())
          .await
          .as_bytes()
          .to_vec(),
      ))
      .try_into()
      .map_err(|err: InvalidHeaderValue| UrlParseError(err.to_string()))?,
    );

    info!("appended server public key");

    Ok((headers, key_pair))
  }
}

/// Type representing the `StreamReader` for `UrlStorage`.
/// Todo, definitely tidy this type...
pub type UrlStreamReader = StreamReader<
  MapErr<
    Pin<Box<dyn Stream<Item = result::Result<Bytes, reqwest::Error>> + Send + Sync>>,
    fn(reqwest::Error) -> StorageError,
  >,
  Bytes,
>;

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

impl From<reqwest::Response> for UrlStreamEither {
  fn from(response: reqwest::Response) -> Self {
    let response: Pin<Box<dyn Stream<Item = result::Result<Bytes, reqwest::Error>> + Send + Sync>> =
      Box::pin(response.bytes_stream());
    let stream_reader: UrlStreamReader = StreamReader::new(
      response.map_err(|err| ResponseError(format!("reading body from response: {}", err))),
    );

    Self::A(stream_reader)
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
    head_output: &mut Option<&mut HeadOutput>,
  ) -> Result<Self::Streamable> {
    info!("Getting underlying file");
    let key = key.as_ref().to_string();
    debug!(calling_from = ?self, key, "getting file with key {:?}", key);

    match KeyType::from_ending(&key) {
      KeyType::File => {
        #[cfg(feature = "crypt4gh")]
        if options.object_type().is_crypt4gh() {
          let (mut headers, key_pair) = self
            .update_headers(options.object_type(), options.request_headers())
            .await?;
          {
            // Additional length for the header.
            let output_headers = head_output
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
              let range = options.range();
              let range = range
                .clone()
                .convert_to_crypt4gh_ranges(crypt4gh_header_length, file_size);

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

          info!("Sending to storage backend with headers: {:#?}", headers);

          let response = self.get_header(key.to_string(), &headers).await?;

          let crypt4gh_keys = Keys {
            method: 0,
            privkey: key_pair.private_key().clone().0,
            recipient_pubkey: key_pair.public_key().clone().into_inner(),
          };

          let response: Pin<
            Box<dyn Stream<Item = result::Result<Bytes, reqwest::Error>> + Send + Sync>,
          > = Box::pin(response.bytes_stream());
          let stream_reader: UrlStreamReader = StreamReader::new(
            response.map_err(|err| ResponseError(format!("reading body from response: {}", err))),
          );

          info!("got stream reader");

          let mut reader = Builder::default().build_with_reader(stream_reader, vec![crypt4gh_keys]);

          reader
            .read_header()
            .await
            .map_err(|err| UrlParseError(err.to_string()))?;

          // Additional length for the header.
          let client_additional_bytes: Option<u64> = head_output
            .as_ref()
            .and_then(|output| output.response_headers())
            .and_then(|headers| {
              headers
                .get(CLIENT_ADDITIONAL_BYTES)
                .or_else(|| headers.get(SERVER_ADDITIONAL_BYTES))
            })
            .and_then(|length| length.to_str().ok())
            .and_then(|length| length.parse().ok());

          // Convert back to the original file size for the rest of the code.
          head_output.iter_mut().try_for_each(|output| {
            let original_file_size = to_unencrypted_file_size(
              output.content_length,
              client_additional_bytes.unwrap_or_else(|| reader.header_size().unwrap_or_default()),
            );

            output.content_length = original_file_size;

            let header_content_length = HeaderValue::from_str(&original_file_size.to_string())
              .map_err(|err| UrlParseError(err.to_string()))?;
            output.response_headers.iter_mut().for_each(|header| {
              header
                .get_mut(CONTENT_LENGTH)
                .iter_mut()
                .for_each(|header| **header = header_content_length.clone())
            });

            Ok::<_, StorageError>(())
          })?;

          info!(
            "additional bytes to return to client: {:#?}",
            client_additional_bytes
          );

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
      KeyType::Index => {
        #[cfg(feature = "crypt4gh")]
        if options.object_type().is_crypt4gh() {
          let (headers, _) = self
            .update_headers(options.object_type(), options.request_headers())
            .await?;
          {
            return Ok(self.get_index(key.to_string(), &headers).await?.into());
          }
        }

        Ok(
          self
            .get_index(key.to_string(), options.request_headers())
            .await?
            .into(),
        )
      }
    }
  }

  #[instrument(level = "trace", skip(self))]
  async fn range_url<K: AsRef<str> + Send + Debug>(
    &self,
    key: K,
    options: RangeUrlOptions<'_>,
  ) -> Result<HtsGetUrl> {
    info!("creating range urls");
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
    info!("getting head");
    let key = key.as_ref();

    #[allow(unused_mut)]
    let mut headers = options.request_headers().clone();
    #[cfg(feature = "crypt4gh")]
    if options.object_type().is_crypt4gh() {
      let (updated_headers, _) = self.update_headers(options.object_type(), &headers).await?;
      headers = updated_headers;
    }

    let head = self.head_key(key, &headers).await?;

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
    info!("updating bytes positions");

    match reader {
      #[cfg(feature = "crypt4gh")]
      UrlStreamEither::B(reader)
        if positions_options.object_type.is_crypt4gh()
          && positions_options
            .object_type
            .send_encrypted_to_client()
            .is_some_and(|send_encrypted_to_client| send_encrypted_to_client) =>
      {
        let Crypt4GHReader {
          reader,
          client_additional_bytes,
        } = reader;

        let keys = reader
          .keys()
          .first()
          .ok_or_else(|| UrlParseError("missing crypt4gh keys from reader".to_string()))?;
        let file_size = positions_options.file_size();

        info!("got keys from reader");

        let client_additional_bytes = if let Some(bytes) = client_additional_bytes {
          bytes
        } else {
          reader
            .header_size()
            .ok_or_else(|| UrlParseError("crypt4gh header has not been read".to_string()))?
        };

        // Convert back to an encrypted file size for encrypted byte ranges.
        let file_size = to_encrypted_file_size(file_size, client_additional_bytes);

        info!("got client additional bytes from reader");

        let client_public_key =
          Self::decode_public_key(positions_options.headers, CLIENT_PUBLIC_KEY_NAME)?;

        info!("decoded client public key: {:#?}", client_public_key);

        let client_public_key = read_public_key(client_public_key)
          .await
          .map_err(|err| UrlParseError(format!("failed to parse client public key: {}", err)))?;

        info!("got client public key: {:#?}", client_public_key);

        // Need to work from the context of defined start and end ranges.
        let positions = positions_options
          .positions
          .clone()
          .into_iter()
          .map(|mut pos| {
            if pos.start.is_none() {
              pos.start = Some(0);
            }
            if pos.end.is_none() {
              pos.end = Some(file_size);
            }

            pos
          })
          .collect::<Vec<BytesPosition>>();

        let unencrypted_positions = BytesPosition::merge_all(positions.clone());
        let clamped_positions = BytesPosition::merge_all(
          positions
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
          client_public_key,
        )?;

        info!("created edit list");

        let encrypted_positions = BytesPosition::merge_all(
          positions
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

        info!("data blocks returned: {:#?}", blocks);

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
  use std::str::FromStr;

  use htsget_config::resolver::object::ObjectType;
  use htsget_test::http::server::with_test_server;
  use http::header::AUTHORIZATION;
  use http::{HeaderName, HeaderValue};
  use tokio::io::AsyncReadExt;
  #[cfg(feature = "crypt4gh")]
  use {
    crate::htsget::from_storage::HtsGetFromStorage,
    crate::htsget::HtsGet,
    crate::Response as HtsgetResponse,
    async_crypt4gh::KeyPair,
    htsget_config::tls::crypt4gh::Crypt4GHKeyPair,
    htsget_config::types::Class::{Body, Header},
    htsget_config::types::Request as HtsgetRequest,
    htsget_config::types::{Format, Query, Url},
    htsget_test::crypt4gh::get_encryption_keys,
    htsget_test::http::default_dir,
    htsget_test::http::test_bam_crypt4gh_byte_ranges,
    htsget_test::http::test_parsable_byte_ranges,
    htsget_test::http::{get_byte_ranges_from_url_storage_response, parse_as_bgzf},
    http::header::USER_AGENT,
  };

  use htsget_config::types::Headers;

  use crate::storage::local::tests::create_local_test_files;

  use super::*;

  #[test]
  fn get_url_from_key() {
    let storage = UrlStorage::new(
      test_client(),
      endpoints_test(),
      Uri::from_str("https://localhost:8080").unwrap(),
      true,
      Some("user-agent".to_string()),
      &Default::default(),
      #[cfg(feature = "crypt4gh")]
      default_key_gen(),
    )
    .unwrap();

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
      Some("user-agent".to_string()),
      &Default::default(),
      #[cfg(feature = "crypt4gh")]
      default_key_gen(),
    )
    .unwrap();

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
        Some("user-agent".to_string()),
        &Default::default(),
        #[cfg(feature = "crypt4gh")]
        default_key_gen(),
      )
      .unwrap();

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);

      let response = String::from_utf8(
        storage
          .send_request(
            "assets/key1",
            headers,
            Method::GET,
            &Uri::from_str(&url).unwrap(),
          )
          .await
          .unwrap()
          .bytes()
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
        Some("user-agent".to_string()),
        &Default::default(),
        #[cfg(feature = "crypt4gh")]
        default_key_gen(),
      )
      .unwrap();

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);

      let response = String::from_utf8(
        storage
          .get_header("assets/key1", headers)
          .await
          .unwrap()
          .bytes()
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
        Some("user-agent".to_string()),
        &Default::default(),
        #[cfg(feature = "crypt4gh")]
        default_key_gen(),
      )
      .unwrap();

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
        Some("user-agent".to_string()),
        &Default::default(),
        #[cfg(feature = "crypt4gh")]
        default_key_gen(),
      )
      .unwrap();

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);
      let object_type = Default::default();
      let options = GetOptions::new_with_default_range(headers, &object_type);

      let mut reader = storage
        .get("assets/key1", options, &mut None)
        .await
        .unwrap();

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
        Some("user-agent".to_string()),
        &Default::default(),
        #[cfg(feature = "crypt4gh")]
        default_key_gen(),
      )
      .unwrap();

      let mut headers = HeaderMap::default();
      let object_type = Default::default();
      let options = test_range_options(&mut headers, &object_type);

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
        Some("user-agent".to_string()),
        &Default::default(),
        #[cfg(feature = "crypt4gh")]
        default_key_gen(),
      )
      .unwrap();

      let mut headers = HeaderMap::default();
      let headers = test_headers(&mut headers);
      let object_type = Default::default();
      let options = HeadOptions::new(headers, &object_type);

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
      Some("user-agent".to_string()),
      &Default::default(),
      #[cfg(feature = "crypt4gh")]
      default_key_gen(),
    )
    .unwrap();

    let mut headers = HeaderMap::default();
    let object_type = Default::default();
    let options = test_range_options(&mut headers, &object_type);

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
      Some("user-agent".to_string()),
      &Default::default(),
      #[cfg(feature = "crypt4gh")]
      default_key_gen(),
    )
    .unwrap();

    let mut headers = HeaderMap::default();
    let object_type = Default::default();
    let options = test_range_options(&mut headers, &object_type);

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
      Some("user-agent".to_string()),
      &Default::default(),
      #[cfg(feature = "crypt4gh")]
      default_key_gen(),
    )
    .unwrap();

    let mut headers = HeaderMap::default();
    let object_type = Default::default();
    let options = test_range_options(&mut headers, &object_type);

    assert_eq!(
      storage.range_url("assets/key1", options,).await.unwrap(),
      HtsGetUrl::new("https://localhost:8081/assets/key1")
    );
  }

  #[cfg(feature = "crypt4gh")]
  #[tokio::test]
  async fn test_endpoints_with_real_file() {
    with_url_test_server(|url| async move {
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

      let storage = UrlStorage::new(
        test_client(),
        endpoints_from_url_with_path(&url),
        Uri::from_str("http://example.com").unwrap(),
        true,
        Some("user-agent".to_string()),
        &query,
        #[cfg(feature = "crypt4gh")]
        default_key_gen(),
      )
      .unwrap();

      let searcher = HtsGetFromStorage::new(storage);
      let response = searcher.search(query.clone()).await;

      let expected_response = Ok(expected_bam_response());
      assert_eq!(response, expected_response);

      let (bytes, _) = get_byte_ranges_from_url_storage_response(
        response.unwrap(),
        default_dir().join("data/bam/htsnexus_test_NA12878.bam"),
      )
      .await;

      parse_as_bgzf(bytes.clone()).await;
    })
    .await;
  }

  #[cfg(feature = "crypt4gh")]
  #[tokio::test]
  async fn test_endpoints_with_real_file_encrypted() {
    with_url_test_server(|url| async move {
      let mut key_gen = default_key_gen();
      key_gen
        .expect_edit_list()
        .times(1)
        .returning(|_, _, _, _, _| Ok(expected_edit_list()));

      let (_, public_key) = get_encryption_keys().await;
      let mut header_map = HeaderMap::default();
      let public_key = general_purpose::STANDARD.encode(public_key);
      test_headers(&mut header_map);
      header_map.append(
        HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
        HeaderValue::from_str(&public_key).unwrap(),
      );
      header_map.append(
        HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
        HeaderValue::from_str("client-user-agent").unwrap(),
      );

      let request =
        HtsgetRequest::new_with_id("htsnexus_test_NA12878".to_string()).with_headers(header_map);
      let query = Query::new(
        "htsnexus_test_NA12878",
        Format::Bam,
        request,
        ObjectType::GenerateKeys {
          send_encrypted_to_client: true,
        },
      )
      .with_reference_name("11")
      .with_start(5015000)
      .with_end(5050000);

      let storage = UrlStorage::new(
        test_client(),
        endpoints_from_url_with_path(&url),
        Uri::from_str("http://example.com").unwrap(),
        true,
        Some("user-agent".to_string()),
        &query,
        key_gen,
      )
      .unwrap();

      let searcher = HtsGetFromStorage::new(storage);
      let response = searcher.search(query.clone()).await.unwrap();

      assert_encrypted_endpoints(&public_key, response).await;
    })
    .await;
  }

  #[cfg(feature = "crypt4gh")]
  #[tokio::test]
  async fn test_endpoints_with_encrypted_file_unencrypted_to_client() {
    with_url_test_server(|url| async move {
      let key_gen = default_key_gen();

      let (_, public_key) = get_encryption_keys().await;
      let mut header_map = HeaderMap::default();
      let public_key = general_purpose::STANDARD.encode(public_key);
      test_headers(&mut header_map);
      header_map.append(
        HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
        HeaderValue::from_str(&public_key).unwrap(),
      );
      header_map.append(
        HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
        HeaderValue::from_str("client-user-agent").unwrap(),
      );

      let request =
        HtsgetRequest::new_with_id("htsnexus_test_NA12878".to_string()).with_headers(header_map);
      let query = Query::new(
        "htsnexus_test_NA12878",
        Format::Bam,
        request,
        ObjectType::GenerateKeys {
          send_encrypted_to_client: false,
        },
      )
      .with_reference_name("11")
      .with_start(5015000)
      .with_end(5050000);

      let storage = UrlStorage::new(
        test_client(),
        endpoints_from_url_with_path(&url),
        Uri::from_str("http://example.com").unwrap(),
        true,
        Some("user-agent".to_string()),
        &query,
        key_gen,
      )
      .unwrap();

      let searcher = HtsGetFromStorage::new(storage);
      let response = searcher.search(query.clone()).await.unwrap();

      let mut expected_response = expected_bam_response();
      expected_response.urls.iter_mut().for_each(|url| {
        url.headers.iter_mut().for_each(|header| {
          *header = header
            .clone()
            .with_header(CLIENT_PUBLIC_KEY_NAME, public_key.clone())
            .with_header(USER_AGENT.to_string(), "client-user-agent")
        })
      });

      assert_eq!(response, expected_response);

      let (bytes, _) = get_byte_ranges_from_url_storage_response(
        response,
        default_dir().join("data/bam/htsnexus_test_NA12878.bam"),
      )
      .await;

      parse_as_bgzf(bytes).await;
    })
    .await;
  }

  #[cfg(feature = "crypt4gh")]
  #[tokio::test]
  async fn test_endpoints_with_predefined_key_pair() {
    with_url_test_server(|url| async move {
      let mut key_gen = Encrypt::default();
      key_gen
        .expect_edit_list()
        .times(1)
        .returning(|_, _, _, _, _| Ok(expected_edit_list()));

      let (_, public_key) = get_encryption_keys().await;
      let mut header_map = HeaderMap::default();
      let public_key = general_purpose::STANDARD.encode(public_key);
      test_headers(&mut header_map);
      header_map.append(
        HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
        HeaderValue::from_str(&public_key).unwrap(),
      );
      header_map.append(
        HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
        HeaderValue::from_str("client-user-agent").unwrap(),
      );

      let request =
        HtsgetRequest::new_with_id("htsnexus_test_NA12878".to_string()).with_headers(header_map);
      let query = Query::new(
        "htsnexus_test_NA12878",
        Format::Bam,
        request,
        ObjectType::Crypt4GH {
          crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
          send_encrypted_to_client: true,
        },
      )
      .with_reference_name("11")
      .with_start(5015000)
      .with_end(5050000);

      let storage = UrlStorage::new(
        test_client(),
        endpoints_from_url_with_path(&url),
        Uri::from_str("http://example.com").unwrap(),
        true,
        Some("user-agent".to_string()),
        &query,
        key_gen,
      )
      .unwrap();

      let searcher = HtsGetFromStorage::new(storage);
      let response = searcher.search(query.clone()).await.unwrap();

      assert_encrypted_endpoints(&public_key, response).await;
    })
    .await;
  }

  #[cfg(feature = "crypt4gh")]
  #[tokio::test]
  async fn test_endpoints_with_full_file_encrypted() {
    with_url_test_server(|url| async move {
      let mut key_gen = Encrypt::default();
      key_gen
        .expect_edit_list()
        .times(1)
        .returning(|_, _, _, _, _| {
          Ok((
            vec![99, 114, 121, 112, 116, 52, 103, 104, 1, 0, 0, 0, 2, 0, 0, 0],
            vec![
              92, 0, 0, 0, 0, 0, 0, 0, 56, 44, 122, 180, 24, 116, 207, 149, 165, 49, 204, 77, 224,
              136, 232, 121, 209, 249, 23, 51, 120, 2, 187, 147, 82, 227, 232, 32, 17, 223, 7, 38,
              137, 197, 83, 68, 73, 33, 229, 38, 173, 186, 106, 216, 22, 90, 243, 19, 191, 45, 212,
              253, 97, 151, 103, 27, 151, 29, 169, 155, 208, 93, 197, 217, 40, 133, 166, 160, 125,
              43, 82, 75, 1, 20, 104, 45, 116, 193, 165, 160, 189, 186, 146, 175,
            ],
          ))
        });

      let (_, public_key) = get_encryption_keys().await;
      let mut header_map = HeaderMap::default();
      let public_key = general_purpose::STANDARD.encode(public_key);
      test_headers(&mut header_map);
      header_map.append(
        HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
        HeaderValue::from_str(&public_key).unwrap(),
      );
      header_map.append(
        HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
        HeaderValue::from_str("client-user-agent").unwrap(),
      );

      let request =
        HtsgetRequest::new_with_id("htsnexus_test_NA12878".to_string()).with_headers(header_map);
      let query = Query::new(
        "htsnexus_test_NA12878",
        Format::Bam,
        request,
        ObjectType::Crypt4GH {
          crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
          send_encrypted_to_client: true,
        },
      );

      let storage = UrlStorage::new(
        test_client(),
        endpoints_from_url_with_path(&url),
        Uri::from_str("http://example.com").unwrap(),
        true,
        Some("user-agent".to_string()),
        &query,
        key_gen,
      )
      .unwrap();

      let searcher = HtsGetFromStorage::new(storage);
      let response = searcher.search(query.clone()).await.unwrap();

      let expected_response = HtsgetResponse::new(
        Format::Bam,
        vec![
          // header info
          Url::new("data:;base64,Y3J5cHQ0Z2gBAAAAAgAAAA=="),
          // original header
          Url::new("http://example.com/htsnexus_test_NA12878.bam.c4gh").with_headers(
            Headers::default()
              .with_header("authorization", "secret")
              .with_header(CLIENT_PUBLIC_KEY_NAME, public_key.clone())
              .with_header("Range", format!("bytes={}-{}", 16, 123))
              .with_header(USER_AGENT.to_string(), "client-user-agent"),
          ),
          // edit list packet
          Url::new(
            "data:;base64,XAAAAAAAAAA4LHq0GHTPlaUxzE3giOh50fkXM3gCu5NS4+ggEd8HJonFU0RJIeUmrbpq2\
            BZa8xO/LdT9YZdnG5cdqZvQXcXZKIWmoH0rUksBFGgtdMGloL26kq8=",
          ),
          Url::new("http://example.com/htsnexus_test_NA12878.bam.c4gh").with_headers(
            Headers::default()
              .with_header("authorization", "secret")
              .with_header(CLIENT_PUBLIC_KEY_NAME, public_key.clone())
              .with_header("Range", format!("bytes={}-{}", 124, 2598043 - 1))
              .with_header(USER_AGENT.to_string(), "client-user-agent"),
          ),
        ],
      );

      assert_eq!(response, expected_response);

      let (bytes, _) = get_byte_ranges_from_url_storage_response(
        response,
        default_dir().join("data/crypt4gh/htsnexus_test_NA12878.bam.c4gh"),
      )
      .await;

      let (expected_bytes, _) = get_byte_ranges_from_url_storage_response(
        HtsgetResponse::new(
          Format::Bam,
          vec![
            Url::new("http://example.com/htsnexus_test_NA12878.bam").with_headers(
              Headers::default()
                .with_header("authorization", "secret")
                .with_header("Range", "bytes=0-2596798"),
            ),
          ],
        ),
        default_dir().join("data/bam/htsnexus_test_NA12878.bam"),
      )
      .await;

      test_bam_crypt4gh_byte_ranges(bytes.clone(), expected_bytes).await;
      test_parsable_byte_ranges(bytes.clone(), Format::Bam, Body).await;
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
        132, 0, 0, 0, 0, 0, 0, 0, 56, 44, 122, 180, 24, 116, 207, 149, 165, 49, 204, 77, 224, 136,
        232, 121, 209, 249, 23, 51, 120, 2, 187, 147, 82, 227, 232, 32, 17, 223, 7, 38, 34, 167,
        71, 22, 226, 141, 116, 29, 102, 158, 147, 237, 135, 239, 3, 75, 15, 202, 173, 254, 237, 63,
        4, 74, 55, 123, 247, 21, 64, 80, 22, 138, 80, 64, 123, 116, 45, 229, 168, 155, 206, 72,
        114, 91, 7, 157, 53, 64, 129, 126, 191, 28, 135, 43, 222, 239, 224, 44, 33, 236, 253, 227,
        238, 111, 15, 132, 138, 99, 251, 156, 186, 26, 98, 81, 117, 63, 75, 17, 133, 22, 24, 98,
        78, 61, 153, 239, 164, 230, 224, 120, 159, 111,
      ],
    )
  }

  #[cfg(feature = "crypt4gh")]
  fn default_key_gen() -> Encrypt {
    let mut key_gen = Encrypt::default();
    key_gen
      .expect_generate_key_pair()
      .times(1)
      .returning(|| Ok(expected_key_pair()));
    key_gen
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
            .with_header("Range", format!("bytes={}-{}", 16, 123))
            .with_header(USER_AGENT.to_string(), "client-user-agent"),
        ),
        // edit list packet
        Url::new(
          "data:;base64,hAAAAAAAAAA4LHq0GHTPlaUxzE3giOh50fkXM3gCu5NS4+ggEd8HJiKnRxbijXQdZp6T7Yf\
          vA0sPyq3+7T8ESjd79xVAUBaKUEB7dC3lqJvOSHJbB501QIF+vxyHK97v4Cwh7P3j7m8PhIpj+5y6GmJRdT9LEYUW\
          GGJOPZnvpObgeJ9v",
        ),
        Url::new("http://example.com/htsnexus_test_NA12878.bam.c4gh").with_headers(
          Headers::default()
            .with_header("authorization", "secret")
            .with_header(CLIENT_PUBLIC_KEY_NAME, public_key)
            .with_header("Range", format!("bytes={}-{}", 124, 124 + 65564 - 1))
            .with_header(USER_AGENT.to_string(), "client-user-agent"),
        ),
        Url::new("http://example.com/htsnexus_test_NA12878.bam.c4gh").with_headers(
          Headers::default()
            .with_header("authorization", "secret")
            .with_header(CLIENT_PUBLIC_KEY_NAME, public_key)
            .with_header(
              "Range",
              format!("bytes={}-{}", 124 + 196692, 124 + 1114588 - 1),
            )
            .with_header(USER_AGENT.to_string(), "client-user-agent"),
        ),
        Url::new("http://example.com/htsnexus_test_NA12878.bam.c4gh").with_headers(
          Headers::default()
            .with_header("authorization", "secret")
            .with_header(CLIENT_PUBLIC_KEY_NAME, public_key)
            .with_header("Range", format!("bytes={}-{}", 124 + 2556996, 2598043 - 1))
            .with_header(USER_AGENT.to_string(), "client-user-agent"),
        ),
      ],
    );

    assert_eq!(response, expected_response);

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/htsnexus_test_NA12878.bam.c4gh"),
    )
    .await;

    let (expected_bytes, _) = get_byte_ranges_from_url_storage_response(
      expected_bam_response(),
      default_dir().join("data/bam/htsnexus_test_NA12878.bam"),
    )
    .await;

    test_bam_crypt4gh_byte_ranges(bytes.clone(), expected_bytes).await;
    test_parsable_byte_ranges(bytes.clone(), Format::Bam, Body).await;
  }

  #[cfg(feature = "crypt4gh")]
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

  fn test_client() -> Client {
    ClientBuilder::new().build().unwrap()
  }

  pub(crate) async fn with_url_test_server<F, Fut>(test: F)
  where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = ()>,
  {
    let (_, base_path) = create_local_test_files().await;
    with_test_server(base_path.path(), test).await;
  }

  fn test_headers(headers: &mut HeaderMap) -> &HeaderMap {
    headers.append(
      HeaderName::from_str(AUTHORIZATION.as_str()).unwrap(),
      HeaderValue::from_str("secret").unwrap(),
    );
    headers
  }

  fn test_range_options<'a>(
    headers: &'a mut HeaderMap,
    object_type: &'a ObjectType,
  ) -> RangeUrlOptions<'a> {
    let headers = test_headers(headers);
    let options = RangeUrlOptions::new_with_default_range(headers, object_type);

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

  #[cfg(feature = "crypt4gh")]
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
