//! Module providing the search capability using BAM/BAI files
//!

use async_trait::async_trait;
use noodles::bam;
use noodles::bam::bai;
use noodles::bam::bai::Index;
use noodles::bgzf;
use noodles::bgzf::VirtualPosition;
use noodles::csi::BinningIndex;
use noodles::csi::binning_index::index::ReferenceSequence;
use noodles::csi::binning_index::index::reference_sequence::index::LinearIndex;
use noodles::sam::Header;
use tokio::io;
use tokio::io::{AsyncRead, BufReader};
use tracing::{instrument, trace};

use crate::Class::Body;
use crate::HtsGetError;
use crate::search::{BgzfSearch, Search, SearchAll, SearchReads};
use crate::{Format, Query, Result};
use htsget_storage::types::BytesPosition;
use htsget_storage::{Storage, Streamable};

type AsyncReader = bam::r#async::io::Reader<bgzf::r#async::io::Reader<Streamable>>;

/// Allows searching through bam files.
pub struct BamSearch {
  storage: Storage,
}

#[async_trait]
impl BgzfSearch<LinearIndex, AsyncReader, Header> for BamSearch {
  #[instrument(level = "trace", skip(self, index))]
  async fn get_byte_ranges_for_unmapped(
    &self,
    query: &Query,
    index: &Index,
  ) -> Result<Vec<BytesPosition>> {
    trace!("getting byte ranges for unmapped reads");
    let last_interval = index.last_first_record_start_position();
    let start = match last_interval {
      Some(start) => start,
      None => {
        VirtualPosition::try_from((self.get_header_end_offset(index).await?, 0)).map_err(|err| {
          HtsGetError::InvalidInput(format!(
            "invalid virtual position generated from header end offset {err}."
          ))
        })?
      }
    };

    Ok(vec![
      BytesPosition::default()
        .with_start(start.compressed())
        .with_end(self.position_at_eof(query).await?)
        .with_class(Body),
    ])
  }

  async fn read_bytes(reader: &mut AsyncReader) -> Option<usize> {
    reader.read_record(&mut Default::default()).await.ok()
  }

  fn virtual_position(&self, reader: &AsyncReader) -> VirtualPosition {
    reader.get_ref().virtual_position()
  }
}

#[async_trait]
impl Search<ReferenceSequence<LinearIndex>, Index, AsyncReader, Header> for BamSearch {
  fn init_reader(inner: Streamable) -> AsyncReader {
    AsyncReader::new(inner)
  }

  async fn read_header(reader: &mut AsyncReader) -> io::Result<Header> {
    reader.read_header().await
  }

  async fn read_index_inner<T: AsyncRead + Unpin + Send>(inner: T) -> io::Result<Index> {
    let mut reader = bai::r#async::io::Reader::new(BufReader::new(inner));
    reader.read_index().await
  }

  #[instrument(level = "trace", skip(self, index, header, query))]
  async fn get_byte_ranges_for_reference_name(
    &mut self,
    reference_name: String,
    index: &Index,
    header: &Header,
    query: &Query,
  ) -> Result<Vec<BytesPosition>> {
    trace!("getting byte ranges for reference name");
    self
      .get_byte_ranges_for_reference_name_reads(&reference_name, index, header, query)
      .await
  }

  fn get_storage(&self) -> &Storage {
    &self.storage
  }

  fn mut_storage(&mut self) -> &mut Storage {
    &mut self.storage
  }

  fn get_format(&self) -> Format {
    Format::Bam
  }
}

#[async_trait]
impl SearchReads<ReferenceSequence<LinearIndex>, Index, AsyncReader, Header> for BamSearch {
  async fn get_reference_sequence_from_name<'a>(
    &self,
    header: &'a Header,
    name: &str,
  ) -> Option<usize> {
    Some(header.reference_sequences().get_index_of(name.as_bytes())?)
  }

  async fn get_byte_ranges_for_unmapped_reads(
    &self,
    query: &Query,
    bai_index: &Index,
  ) -> Result<Vec<BytesPosition>> {
    self.get_byte_ranges_for_unmapped(query, bai_index).await
  }

  async fn get_byte_ranges_for_reference_sequence(
    &mut self,
    ref_seq_id: usize,
    query: &Query,
    index: &Index,
  ) -> Result<Vec<BytesPosition>> {
    self
      .get_byte_ranges_for_reference_sequence_bgzf(query, ref_seq_id, index)
      .await
  }
}

impl BamSearch {
  /// Create the bam search.
  pub fn new(storage: Storage) -> Self {
    Self { storage }
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use super::*;
  #[cfg(feature = "aws")]
  use crate::from_storage::tests::with_aws_storage_fn;
  use crate::from_storage::tests::with_local_storage_fn;
  use crate::{Class::Body, Class::Header, Headers, HtsGetError::NotFound, Response, Url};
  use htsget_test::http::concat::ConcatResponse;
  use std::future::Future;
  #[cfg(feature = "experimental")]
  use {
    crate::from_storage::tests::with_local_storage_c4gh,
    htsget_storage::c4gh::storage::C4GHStorage, htsget_test::c4gh::get_decryption_keys,
  };

  const DATA_LOCATION: &str = "data/bam";
  const INDEX_FILE_LOCATION: &str = "htsnexus_test_NA12878.bam.bai";
  pub(crate) const BAM_FILE_NAME: &str = "htsnexus_test_NA12878.bam";

  #[tokio::test]
  async fn search_all_reads() {
    with_local_storage(|storage| async move {
      let mut search = BamSearch::new(storage);
      let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-2596798")),
        ],
      ));
      assert_eq!(response, expected_response);

      Some((BAM_FILE_NAME.to_string(), (response.unwrap(), Body).into()))
    })
    .await;
  }

  #[tokio::test]
  async fn search_unmapped_reads() {
    with_local_storage(|storage| async move {
      let mut search = BamSearch::new(storage);
      let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam)
        .with_reference_name("*");
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-4667"))
            .with_class(Header),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=2060795-2596798"))
            .with_class(Body),
        ],
      ));
      assert_eq!(response, expected_response);

      Some((BAM_FILE_NAME.to_string(), (response.unwrap(), Body).into()))
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_without_seq_range_chr11() {
    with_local_storage(|storage| async move {
      let mut search = BamSearch::new(storage);
      let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam)
        .with_reference_name("11");
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-996014")),
          expected_eof_url().set_class(None),
        ],
      ));
      assert_eq!(response, expected_response);

      Some((BAM_FILE_NAME.to_string(), (response.unwrap(), Body).into()))
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_without_seq_range_chr20() {
    with_local_storage(|storage| async move {
      let mut search = BamSearch::new(storage);
      let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam)
        .with_reference_name("20");
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-4667"))
            .with_class(Header),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=977196-2128165"))
            .with_class(Body),
          expected_eof_url(),
        ],
      ));
      assert_eq!(response, expected_response);

      Some((BAM_FILE_NAME.to_string(), (response.unwrap(), Body).into()))
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_with_seq_range() {
    with_local_storage(|storage| async move {
      let mut search = BamSearch::new(storage);
      let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam)
        .with_reference_name("11")
        .with_start(5015000)
        .with_end(5050000);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-4667"))
            .with_class(Header),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=256721-647345"))
            .with_class(Body),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=824361-842100"))
            .with_class(Body),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=977196-996014"))
            .with_class(Body),
          expected_eof_url(),
        ],
      ));
      assert_eq!(response, expected_response);

      Some((BAM_FILE_NAME.to_string(), (response.unwrap(), Body).into()))
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_no_end_position() {
    with_local_storage(|storage| async move {
      let mut search = BamSearch::new(storage);
      let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam)
        .with_reference_name("11")
        .with_start(5015000);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-4667"))
            .with_class(Header),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=256721-996014"))
            .with_class(Body),
          expected_eof_url(),
        ],
      ));
      assert_eq!(response, expected_response);

      Some((BAM_FILE_NAME.to_string(), (response.unwrap(), Body).into()))
    })
    .await;
  }

  #[tokio::test]
  async fn search_many_response_urls() {
    with_local_storage(|storage| async move {
      let mut search = BamSearch::new(storage);
      let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam)
        .with_reference_name("11")
        .with_start(4999976)
        .with_end(5003981);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-273085")),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=499249-574358")),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=627987-647345")),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=824361-842100")),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=977196-996014")),
          expected_eof_url().set_class(None),
        ],
      ));
      assert_eq!(response, expected_response);

      Some((BAM_FILE_NAME.to_string(), (response.unwrap(), Body).into()))
    })
    .await
  }

  #[tokio::test]
  async fn search_no_gzi() {
    with_local_storage_fn(
      |storage| async move {
        let mut search = BamSearch::new(storage);
        let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam)
          .with_reference_name("11")
          .with_start(5015000)
          .with_end(5050000);
        let response = search.search(query).await;
        println!("{response:#?}");

        let expected_response = Ok(Response::new(
          Format::Bam,
          vec![
            Url::new(expected_url())
              .with_headers(Headers::default().with_header("Range", "bytes=0-4667"))
              .with_class(Header),
            Url::new(expected_url())
              .with_headers(Headers::default().with_header("Range", "bytes=256721-1065951"))
              .with_class(Body),
            expected_eof_url(),
          ],
        ));
        assert_eq!(response, expected_response);

        Some((BAM_FILE_NAME.to_string(), (response.unwrap(), Body).into()))
      },
      DATA_LOCATION,
      &[BAM_FILE_NAME, INDEX_FILE_LOCATION],
    )
    .await
  }

  #[tokio::test]
  async fn search_header() {
    with_local_storage(|storage| async move {
      let mut search = BamSearch::new(storage);
      let query =
        Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam).with_class(Header);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-4667"))
            .with_class(Header),
        ],
      ));
      assert_eq!(response, expected_response);

      Some((
        BAM_FILE_NAME.to_string(),
        (response.unwrap(), Header).into(),
      ))
    })
    .await;
  }

  #[tokio::test]
  async fn search_header_with_no_mapped_reads() {
    with_local_storage(|storage| async move {
      let mut search = BamSearch::new(storage);
      let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam)
        .with_reference_name("22");
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-4667"))
            .with_class(Header),
          expected_eof_url(),
        ],
      ));
      assert_eq!(response, expected_response);

      Some((BAM_FILE_NAME.to_string(), (response.unwrap(), Body).into()))
    })
    .await;
  }

  #[tokio::test]
  async fn search_header_with_non_existent_reference_name() {
    with_local_storage(|storage| async move {
      let mut search = BamSearch::new(storage);
      let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam)
        .with_reference_name("25");
      let response = search.search(query).await;
      println!("{response:#?}");

      assert!(matches!(response, Err(NotFound(_))));

      None
    })
    .await;
  }

  #[tokio::test]
  async fn search_non_existent_id_reference_name() {
    with_local_storage_fn(
      |storage| async move {
        let mut search = BamSearch::new(storage);
        let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam);
        let response = search.search(query).await;
        assert!(matches!(response, Err(NotFound(_))));

        None
      },
      DATA_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[tokio::test]
  async fn search_non_existent_id_all_reads() {
    with_local_storage_fn(
      |storage| async move {
        let mut search = BamSearch::new(storage);
        let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam)
          .with_reference_name("20");
        let response = search.search(query).await;
        assert!(matches!(response, Err(NotFound(_))));

        None
      },
      DATA_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[tokio::test]
  async fn search_non_existent_id_header() {
    with_local_storage_fn(
      |storage| async move {
        let mut search = BamSearch::new(storage);
        let query =
          Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam).with_class(Header);
        let response = search.search(query).await;
        assert!(matches!(response, Err(NotFound(_))));

        None
      },
      DATA_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[tokio::test]
  async fn get_header_end_offset() {
    with_local_storage_fn(
      |storage| async move {
        let search = BamSearch::new(storage);
        let query =
          Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam).with_class(Header);

        let index = search.read_index(&query).await.unwrap();
        let response = search.get_header_end_offset(&index).await;

        assert_eq!(response, Ok(70204));

        None
      },
      DATA_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[cfg(feature = "aws")]
  #[tokio::test]
  async fn search_non_existent_id_reference_name_aws() {
    with_aws_storage_fn(
      |storage| async move {
        let mut search = BamSearch::new(storage);
        let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam);
        let response = search.search(query).await;
        assert!(response.is_err());

        None
      },
      DATA_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[cfg(feature = "aws")]
  #[tokio::test]
  async fn search_non_existent_id_all_reads_aws() {
    with_aws_storage_fn(
      |storage| async move {
        let mut search = BamSearch::new(storage);
        let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam)
          .with_reference_name("20");
        let response = search.search(query).await;
        assert!(response.is_err());

        None
      },
      DATA_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[cfg(feature = "aws")]
  #[tokio::test]
  async fn search_non_existent_id_header_aws() {
    with_aws_storage_fn(
      |storage| async move {
        let mut search = BamSearch::new(storage);
        let query =
          Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam).with_class(Header);
        let response = search.search(query).await;
        assert!(response.is_err());

        None
      },
      DATA_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[cfg(feature = "experimental")]
  #[tokio::test]
  async fn search_all_c4gh() {
    with_local_storage_c4gh(|storage| async move {
      let storage = C4GHStorage::new(get_decryption_keys().await, storage);
      let mut search = BamSearch::new(Storage::new(storage));
      let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam);
      let response = search.search(query).await.unwrap();

      println!("{response:#?}");

      Some((
        "htsnexus_test_NA12878.bam.c4gh".to_string(),
        (response, Body).into(),
      ))
    })
    .await;
  }

  #[cfg(feature = "experimental")]
  #[tokio::test]
  async fn search_all_range_c4gh() {
    with_local_storage_c4gh(|storage| async move {
      let storage = C4GHStorage::new(get_decryption_keys().await, storage);
      let mut search = BamSearch::new(Storage::new(storage));
      let query = Query::new_with_default_request("htsnexus_test_NA12878", Format::Bam)
        .with_reference_name("11")
        .with_start(5015000)
        .with_end(5050000);
      let response = search.search(query).await.unwrap();

      println!("{response:#?}");

      Some((
        "htsnexus_test_NA12878.bam.c4gh".to_string(),
        (response, Body).into(),
      ))
    })
    .await;
  }

  pub(crate) async fn with_local_storage<F, Fut>(test: F)
  where
    F: FnOnce(Storage) -> Fut,
    Fut: Future<Output = Option<(String, ConcatResponse)>>,
  {
    with_local_storage_fn(test, DATA_LOCATION, &[]).await
  }

  pub(crate) fn expected_url() -> String {
    "http://127.0.0.1:8081/htsnexus_test_NA12878.bam".to_string()
  }

  pub(crate) fn expected_eof_url() -> Url {
    Url::new(expected_url())
      .with_headers(Headers::default().with_header("Range", "bytes=2596771-2596798"))
      .with_class(Body)
  }
}
