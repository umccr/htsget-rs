use crate::error::TestError::ConcatResponse as ConcatResponseError;
use crate::error::{Result, TestError};
use base64::engine::general_purpose;
use base64::Engine;
use futures::future::join_all;
use futures::{Stream, TryStreamExt};
use htsget_config::types::{Class, Format, Response, Url};
use http::{HeaderMap, HeaderName, HeaderValue};
use noodles::{bam, bcf, bgzf, cram, fasta, vcf};
use reqwest::Client;
use std::future::Future;
use std::io;
use std::path::Path;
use std::str::FromStr;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

/// A response concatenator which concatenates url tickets.
#[derive(Debug)]
pub struct ConcatResponse {
  response: Response,
  class: Class,
}

impl ConcatResponse {
  /// Create a new response concatenator.
  pub fn new(response: Response, class: Class) -> Self {
    Self { response, class }
  }

  /// Get the inner response.
  pub fn into_inner(self) -> Response {
    self.response
  }

  /// Get the inner response.
  pub fn response(&self) -> &Response {
    &self.response
  }

  /// Concatenate a response into the bytes represented by the url ticket with a file path
  pub async fn concat_from_file_path(self, path: impl AsRef<Path>) -> Result<ReadRecords> {
    let file = File::open(path).await?;
    self.concat_from_file(file).await
  }

  /// Concatenate a response into the bytes represented by the url ticket with file data.
  pub async fn concat_from_file(self, mut file: File) -> Result<ReadRecords> {
    let mut bytes = vec![];
    file.read_to_end(&mut bytes).await?;

    self.concat_from_bytes(bytes.as_slice()).await
  }

  /// Concatentate a response into bytes using a reqwest client.
  pub async fn concat_from_client(self, client: &Client) -> Result<ReadRecords> {
    let merged_bytes = join_all(self.response.urls.into_iter().map(|url| {
      Self::url_to_bytes(url, |url| async move {
        Ok(
          client
            .get(url.url.as_str())
            .headers(HeaderMap::from_iter(
              url
                .headers
                .unwrap_or_default()
                .into_inner()
                .into_iter()
                .map(|(key, value)| {
                  Ok((
                    HeaderName::from_str(&key).map_err(TestError::concat_response)?,
                    HeaderValue::from_str(&value).map_err(TestError::concat_response)?,
                  ))
                })
                .collect::<Result<Vec<(HeaderName, HeaderValue)>>>()?
                .into_iter(),
            ))
            .send()
            .await
            .map_err(TestError::concat_response)?
            .bytes()
            .await
            .map_err(TestError::concat_response)?
            .to_vec(),
        )
      })
    }))
    .await
    .into_iter()
    .collect::<Result<Vec<Vec<u8>>>>()?
    .concat();

    Ok(ReadRecords::new(
      self.response.format,
      self.class,
      merged_bytes,
    ))
  }

  /// Concatenate a response into the bytes represented by the url ticket with bytes data.
  pub async fn concat_from_bytes(self, bytes: &[u8]) -> Result<ReadRecords> {
    let merged_bytes = join_all(self.response.urls.into_iter().map(|url| {
      Self::url_to_bytes(url, |url| async move {
        let headers = url
          .headers
          .ok_or_else(|| ConcatResponseError("missing url headers".to_string()))?
          .into_inner();
        let range = headers
          .get("Range")
          .ok_or_else(|| ConcatResponseError("missing header range".to_string()))?;
        let range = range
          .strip_prefix("bytes=")
          .ok_or_else(|| ConcatResponseError("failed to parse header range bytes".to_string()))?;

        let split: Vec<&str> = range.splitn(2, '-').collect();

        Ok(
          bytes[split[0].parse().map_err(TestError::read_record)?
            ..split[1].parse::<usize>().map_err(TestError::read_record)? + 1]
            .to_vec(),
        )
      })
    }))
    .await
    .into_iter()
    .collect::<Result<Vec<Vec<u8>>>>()?
    .concat();

    Ok(ReadRecords::new(
      self.response.format,
      self.class,
      merged_bytes,
    ))
  }

  /// Convert the url to bytes with a transform function for the range urls.
  pub async fn url_to_bytes<F, Fut>(url: Url, for_range_url: F) -> Result<Vec<u8>>
  where
    F: FnOnce(Url) -> Fut,
    Fut: Future<Output = Result<Vec<u8>>>,
  {
    if let Some(data_uri) = url.url.strip_prefix("data:;base64,") {
      general_purpose::STANDARD
        .decode(data_uri)
        .map_err(TestError::concat_response)
    } else {
      for_range_url(url).await
    }
  }
}

impl From<(Response, Class)> for ConcatResponse {
  fn from((response, class): (Response, Class)) -> Self {
    Self::new(response, class)
  }
}

/// A record reader.
#[derive(Debug)]
pub struct ReadRecords {
  format: Format,
  class: Class,
  merged_bytes: Vec<u8>,
}

impl ReadRecords {
  /// Create a new record reader.
  pub fn new(format: Format, class: Class, merged_bytes: Vec<u8>) -> Self {
    Self {
      format,
      class,
      merged_bytes,
    }
  }

  /// Get the format.
  pub fn format(&self) -> &Format {
    &self.format
  }

  /// Get the format.
  pub fn merged_bytes(&self) -> &[u8] {
    self.merged_bytes.as_slice()
  }

  /// Read records to confirm they are valid.
  pub async fn read_records(self) -> Result<()> {
    match self.format {
      Format::Bam => {
        let mut reader = bam::AsyncReader::new(self.merged_bytes.as_slice());
        let header = reader.read_header().await.map_err(TestError::read_record)?;
        println!("{:#?}", header);

        self.iterate_records(reader.records()).await
      }
      Format::Cram => {
        let mut reader = cram::AsyncReader::new(self.merged_bytes.as_slice());

        reader
          .read_file_definition()
          .await
          .map_err(TestError::read_record)?;
        let repository = fasta::Repository::default();
        let header = reader
          .read_file_header()
          .await
          .map_err(TestError::read_record)?
          .parse()
          .map_err(TestError::read_record)?;
        println!("{:#?}", header);

        self
          .iterate_records(reader.records(&repository, &header))
          .await
      }
      Format::Vcf => {
        let mut reader =
          vcf::AsyncReader::new(bgzf::AsyncReader::new(self.merged_bytes.as_slice()));
        let header = reader.read_header().await.map_err(TestError::read_record)?;
        println!("{header}");

        self.iterate_records(reader.records(&header)).await
      }
      Format::Bcf => {
        let mut reader = bcf::AsyncReader::new(self.merged_bytes.as_slice());
        reader
          .read_file_format()
          .await
          .map_err(TestError::read_record)?;
        reader.read_header().await.map_err(TestError::read_record)?;

        self.iterate_records(reader.lazy_records()).await
      }
    }
  }

  async fn iterate_records<T>(
    &self,
    mut records: impl Stream<Item = io::Result<T>> + Unpin,
  ) -> Result<()> {
    if let Class::Body = self.class {
      let mut total_records = 0;

      while records
        .try_next()
        .await
        .map_err(TestError::read_record)?
        .is_some()
      {
        total_records += 1;
        continue;
      }

      println!("total records read: {}", total_records);
    }

    Ok(())
  }
}
