use base64::engine::general_purpose;
use base64::Engine;
use futures::future::join_all;
use htsget_config::types::{Response, Url};
use http::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Client;
use std::future::Future;
use std::str::FromStr;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

/// A response concatenator which concatenates url tickets.
pub struct ConcatResponse {
  response: Response,
}

impl ConcatResponse {
  /// Create a new response concatenator.
  pub fn new(response: Response) -> Self {
    Self { response }
  }

  /// Get the inner response.
  pub fn into_inner(self) -> Response {
    self.response
  }

  /// Get the inner response.
  pub fn response(&self) -> &Response {
    &self.response
  }

  /// Concatenate a response into the bytes represented by the url ticket with file data.
  pub async fn concat_from_file(self, mut file: File) -> Vec<u8> {
    let mut bytes = vec![];
    file.read_to_end(&mut bytes).await.unwrap();

    self.concat_from_bytes(bytes.as_slice()).await
  }

  pub async fn concat_from_client(self, client: &Client) -> Vec<u8> {
    join_all(self.response.urls.into_iter().map(|url| {
      Self::url_to_bytes(url, |url| async move {
        client
          .get(url.url.as_str())
          .headers(HeaderMap::from_iter(
            url
              .headers
              .unwrap_or_default()
              .into_inner()
              .into_iter()
              .map(|(key, value)| {
                (
                  HeaderName::from_str(&key).unwrap(),
                  HeaderValue::from_str(&value).unwrap(),
                )
              }),
          ))
          .send()
          .await
          .unwrap()
          .bytes()
          .await
          .unwrap()
          .to_vec()
      })
    }))
    .await
    .into_iter()
    .collect::<Vec<Vec<u8>>>()
    .concat()
  }

  /// Concatenate a response into the bytes represented by the url ticket with bytes data.
  pub async fn concat_from_bytes(self, bytes: &[u8]) -> Vec<u8> {
    join_all(self.response.urls.into_iter().map(|url| {
      Self::url_to_bytes(url, |url| async move {
        let headers = url.headers.unwrap().into_inner();
        let range = headers.get("Range").unwrap();
        let range = range.strip_prefix("bytes=").unwrap();

        let split: Vec<&str> = range.splitn(2, '-').collect();
        bytes[split[0].parse().unwrap()..split[1].parse().unwrap()].to_vec()
      })
    }))
    .await
    .into_iter()
    .collect::<Vec<Vec<u8>>>()
    .concat()
  }

  /// Convert the url to bytes with a transform function for the range urls.
  pub async fn url_to_bytes<F, Fut>(url: Url, for_range_url: F) -> Vec<u8>
  where
    F: FnOnce(Url) -> Fut,
    Fut: Future<Output = Vec<u8>>,
  {
    if let Some(data_uri) = url.url.strip_prefix("data:;base64,") {
      general_purpose::STANDARD.decode(data_uri).unwrap()
    } else {
      for_range_url(url).await
    }
  }
}
