use base64::engine::general_purpose;
use base64::Engine;
use futures::future::join_all;
use futures::TryStreamExt;
use htsget_config::types::{Format, Response, Url};
use http::{HeaderMap, HeaderName, HeaderValue};
use noodles::{bam, bcf, bgzf, cram, fasta, vcf};
use reqwest::Client;
use std::future::Future;
use std::path::Path;
use std::str::FromStr;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

/// A response concatenator which concatenates url tickets.
#[derive(Debug)]
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

  /// Concatenate a response into the bytes represented by the url ticket with a file path
  pub async fn concat_from_file_path(self, path: impl AsRef<Path>) -> ReadRecords {
    let file = File::open(path).await.unwrap();
    self.concat_from_file(file).await
  }

  /// Concatenate a response into the bytes represented by the url ticket with file data.
  pub async fn concat_from_file(self, mut file: File) -> ReadRecords {
    let mut bytes = vec![];
    file.read_to_end(&mut bytes).await.unwrap();

    self.concat_from_bytes(bytes.as_slice()).await
  }

  pub async fn concat_from_client(self, client: &Client) -> ReadRecords {
    let merged_bytes = join_all(self.response.urls.into_iter().map(|url| {
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
    .concat();

    ReadRecords::new(self.response.format, merged_bytes)
  }

  /// Concatenate a response into the bytes represented by the url ticket with bytes data.
  pub async fn concat_from_bytes(self, bytes: &[u8]) -> ReadRecords {
    let merged_bytes = join_all(self.response.urls.into_iter().map(|url| {
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
    .concat();

    ReadRecords::new(self.response.format, merged_bytes)
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

impl From<Response> for ConcatResponse {
  fn from(response: Response) -> Self {
    Self::new(response)
  }
}

/// A record reader.
#[derive(Debug)]
pub struct ReadRecords {
  format: Format,
  merged_bytes: Vec<u8>,
}

impl ReadRecords {
  /// Create a new record reader.
  pub fn new(format: Format, merged_bytes: Vec<u8>) -> Self {
    Self {
      format,
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
  pub async fn read_records(self) {
    match self.format {
      Format::Bam => {
        let mut reader =
          bam::AsyncReader::new(bgzf::AsyncReader::new(self.merged_bytes.as_slice()));
        let header = reader.read_header().await.unwrap().parse().unwrap();
        println!("{header}");

        reader.read_reference_sequences().await.unwrap();
        let mut records = reader.records(&header);
        while let Some(record) = records.try_next().await.unwrap() {
          println!("{:#?}", record);
          continue;
        }
      }
      Format::Cram => {
        let mut reader = cram::AsyncReader::new(self.merged_bytes.as_slice());

        reader.read_file_definition().await.unwrap();
        let repository = fasta::Repository::default();
        let header = reader.read_file_header().await.unwrap().parse().unwrap();
        println!("{header}");

        let mut records = reader.records(&repository, &header);
        while let Some(record) = records.try_next().await.unwrap() {
          println!("{:#?}", record);
          continue;
        }
      }
      Format::Vcf => {
        let mut reader =
          vcf::AsyncReader::new(bgzf::AsyncReader::new(self.merged_bytes.as_slice()));
        let header = reader.read_header().await.unwrap();
        println!("{header}");

        let mut records = reader.records(&header);
        while let Some(record) = records.try_next().await.unwrap() {
          println!("{record}");
          continue;
        }
      }
      Format::Bcf => {
        let mut reader =
          bcf::AsyncReader::new(bgzf::AsyncReader::new(self.merged_bytes.as_slice()));
        reader.read_file_format().await.unwrap();
        reader.read_header().await.unwrap();

        let mut records = reader.lazy_records();
        while let Some(record) = records.try_next().await.unwrap() {
          println!("{:#?}", record);
          continue;
        }
      }
    }
  }
}
