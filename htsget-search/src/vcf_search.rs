//! Module providing the search capability using VCF files
//!

use async_trait::async_trait;
use futures_util::stream::FuturesOrdered;
use noodles::bgzf;
use noodles::bgzf::VirtualPosition;
use noodles::csi::binning_index::index::reference_sequence::index::LinearIndex;
use noodles::csi::binning_index::index::ReferenceSequence;
use noodles::csi::BinningIndex;
use noodles::tabix;
use noodles::tabix::Index;
use noodles::vcf;
use noodles::vcf::Header;
use tokio::io;
use tokio::io::AsyncRead;
use tracing::{instrument, trace};

use htsget_config::types::HtsGetError;

use crate::search::{find_first, BgzfSearch, Search};
use crate::{Format, Query, Result};
use htsget_storage::types::BytesPosition;
use htsget_storage::{Storage, Streamable};

type AsyncReader = vcf::AsyncReader<bgzf::AsyncReader<Streamable>>;

/// Allows searching through vcf files.
pub struct VcfSearch {
  storage: Storage,
}

#[async_trait]
impl BgzfSearch<LinearIndex, AsyncReader, Header> for VcfSearch {
  async fn read_bytes(reader: &mut AsyncReader) -> Option<usize> {
    reader.read_record(&mut Default::default()).await.ok()
  }

  fn virtual_position(&self, reader: &AsyncReader) -> VirtualPosition {
    reader.get_ref().virtual_position()
  }
}

#[async_trait]
impl Search<ReferenceSequence<LinearIndex>, Index, AsyncReader, Header> for VcfSearch {
  fn init_reader(inner: Streamable) -> AsyncReader {
    AsyncReader::new(bgzf::AsyncReader::new(inner))
  }

  async fn read_header(reader: &mut AsyncReader) -> io::Result<Header> {
    reader.read_header().await
  }

  async fn read_index_inner<T: AsyncRead + Unpin + Send>(inner: T) -> io::Result<Index> {
    tabix::AsyncReader::new(inner).read_index().await
  }

  #[instrument(level = "trace", skip(self, index, query))]
  async fn get_byte_ranges_for_reference_name(
    &self,
    reference_name: String,
    index: &Index,
    _header: &Header,
    query: &Query,
  ) -> Result<Vec<BytesPosition>> {
    trace!("getting byte ranges for reference name");
    // We are assuming the order of the names and the references sequences
    // in the index is the same
    let mut futures = FuturesOrdered::new();
    for (index, name) in index
      .header()
      .ok_or_else(|| HtsGetError::parse_error("no tabix header found in index"))?
      .reference_sequence_names()
      .iter()
      .enumerate()
    {
      let owned_name = name.to_owned();
      let owned_reference_name = reference_name.clone();
      futures.push_back(tokio::spawn(async move {
        if owned_name == owned_reference_name {
          Some(index)
        } else {
          None
        }
      }));
    }

    let ref_seq_id = find_first(
      &format!("reference name not found in TBI file: {reference_name}"),
      futures,
    )
    .await?;

    let byte_ranges = self
      .get_byte_ranges_for_reference_sequence_bgzf(query, ref_seq_id, index)
      .await?;
    Ok(byte_ranges)
  }

  fn get_storage(&self) -> &Storage {
    &self.storage
  }

  fn mut_storage(&mut self) -> &mut Storage {
    &mut self.storage
  }

  fn get_format(&self) -> Format {
    Format::Vcf
  }
}

impl VcfSearch {
  /// Create the vcf search.
  pub fn new(storage: Storage) -> Self {
    Self { storage }
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use htsget_config::types::Class::Body;
  use htsget_test::http::concat::ConcatResponse;
  use std::future::Future;

  use super::*;
  #[cfg(feature = "s3")]
  use crate::from_storage::tests::with_aws_storage_fn;
  use crate::from_storage::tests::with_local_storage_fn;
  use crate::search::SearchAll;
  use crate::{Class::Header, Headers, HtsGetError::NotFound, Response, Url};
  #[cfg(feature = "experimental")]
  use {
    crate::from_storage::tests::with_local_storage_c4gh,
    htsget_storage::c4gh::storage::C4GHStorage, htsget_test::c4gh::get_decryption_keys,
  };

  const VCF_LOCATION: &str = "data/vcf";
  const INDEX_FILE_LOCATION: &str = "spec-v4.3.vcf.gz.tbi";
  pub(crate) const VCF_FILE_NAME_SPEC: &str = "spec-v4.3.vcf.gz";
  const VCF_FILE_NAME_SAMPLE: &str = "sample1-bcbio-cancer.vcf.gz";

  #[tokio::test]
  async fn search_all_variants() {
    with_local_storage(|storage| async move {
      let mut search = VcfSearch::new(storage);
      let filename = "sample1-bcbio-cancer";
      let query = Query::new_with_default_request(filename, Format::Vcf);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(expected_vcf_response(filename));
      assert_eq!(response, expected_response);

      Some((
        VCF_FILE_NAME_SAMPLE.to_string(),
        (response.unwrap(), Body).into(),
      ))
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_without_seq_range() {
    with_local_storage(|storage| async move {
      let mut search = VcfSearch::new(storage);
      let filename = "spec-v4.3";
      let query = Query::new_with_default_request(filename, Format::Vcf).with_reference_name("20");
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(expected_url(filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-850"))],
      ));
      assert_eq!(response, expected_response);

      Some((
        VCF_FILE_NAME_SPEC.to_string(),
        (response.unwrap(), Body).into(),
      ))
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_with_seq_range() {
    with_local_storage(|storage| async move { test_reference_name_with_seq_range(storage).await })
      .await;
  }

  #[tokio::test]
  async fn search_reference_name_no_end_position() {
    with_local_storage(|storage| async move {
      let mut search = VcfSearch::new(storage);
      let filename = "sample1-bcbio-cancer";
      let query = Query::new_with_default_request(filename, Format::Vcf)
        .with_reference_name("chrM")
        .with_start(151)
        .with_end(153);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(expected_vcf_response(filename));
      assert_eq!(response, expected_response);

      Some((
        VCF_FILE_NAME_SAMPLE.to_string(),
        (response.unwrap(), Body).into(),
      ))
    })
    .await;
  }

  #[tokio::test]
  async fn search_no_gzi() {
    with_local_storage_fn(
      |storage| async move { test_reference_name_with_seq_range(storage).await },
      VCF_LOCATION,
      &[
        "sample1-bcbio-cancer.vcf.gz",
        "sample1-bcbio-cancer.vcf.gz.tbi",
      ],
    )
    .await;
  }

  #[tokio::test]
  async fn search_header() {
    with_local_storage(|storage| async move {
      let mut search = VcfSearch::new(storage);
      let filename = "spec-v4.3";
      let query = Query::new_with_default_request(filename, Format::Vcf).with_class(Header);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(expected_url(filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-822"))
          .with_class(Header)],
      ));
      assert_eq!(response, expected_response);

      Some((
        VCF_FILE_NAME_SPEC.to_string(),
        (response.unwrap(), Header).into(),
      ))
    })
    .await;
  }

  #[tokio::test]
  async fn search_non_existent_id_reference_name() {
    with_local_storage_fn(
      |storage| async move {
        let mut search = VcfSearch::new(storage);
        let query = Query::new_with_default_request("spec-v4.3", Format::Vcf);
        let response = search.search(query).await;
        assert!(matches!(response, Err(NotFound(_))));

        None
      },
      VCF_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[tokio::test]
  async fn search_non_existent_id_all_reads() {
    with_local_storage_fn(
      |storage| async move {
        let mut search = VcfSearch::new(storage);
        let query =
          Query::new_with_default_request("spec-v4.3", Format::Vcf).with_reference_name("chrM");
        let response = search.search(query).await;
        assert!(matches!(response, Err(NotFound(_))));

        None
      },
      VCF_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[tokio::test]
  async fn search_non_existent_id_header() {
    with_local_storage_fn(
      |storage| async move {
        let mut search = VcfSearch::new(storage);
        let query = Query::new_with_default_request("spec-v4.3", Format::Vcf).with_class(Header);
        let response = search.search(query).await;
        assert!(matches!(response, Err(NotFound(_))));

        None
      },
      VCF_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[tokio::test]
  async fn search_header_with_non_existent_reference_name() {
    with_local_storage(|storage| async move {
      let mut search = VcfSearch::new(storage);
      let query =
        Query::new_with_default_request("spec-v4.3", Format::Vcf).with_reference_name("chr1");
      let response = search.search(query).await;
      println!("{response:#?}");

      assert!(matches!(response, Err(NotFound(_))));

      None
    })
    .await;
  }

  #[tokio::test]
  async fn get_header_end_offset() {
    with_local_storage_fn(
      |storage| async move {
        let search = VcfSearch::new(storage);
        let query = Query::new_with_default_request("spec-v4.3", Format::Vcf).with_class(Header);

        let index = search.read_index(&query).await.unwrap();
        let response = search.get_header_end_offset(&index).await;

        assert_eq!(response, Ok(65536));

        None
      },
      VCF_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[cfg(feature = "s3")]
  #[tokio::test]
  async fn search_non_existent_id_reference_name_aws() {
    with_aws_storage_fn(
      |storage| async move {
        let mut search = VcfSearch::new(storage);
        let query = Query::new_with_default_request("spec-v4.3", Format::Vcf);
        let response = search.search(query).await;
        assert!(response.is_err());

        None
      },
      VCF_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[cfg(feature = "s3")]
  #[tokio::test]
  async fn search_non_existent_id_all_reads_aws() {
    with_aws_storage_fn(
      |storage| async move {
        let mut search = VcfSearch::new(storage);
        let query =
          Query::new_with_default_request("spec-v4.3", Format::Vcf).with_reference_name("chrM");
        let response = search.search(query).await;
        assert!(response.is_err());

        None
      },
      VCF_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[cfg(feature = "s3")]
  #[tokio::test]
  async fn search_non_existent_id_header_aws() {
    with_aws_storage_fn(
      |storage| async move {
        let mut search = VcfSearch::new(storage);
        let query = Query::new_with_default_request("spec-v4.3", Format::Vcf).with_class(Header);
        let response = search.search(query).await;
        assert!(response.is_err());

        None
      },
      VCF_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[cfg(feature = "experimental")]
  #[tokio::test]
  async fn search_all_c4gh() {
    with_local_storage_c4gh(|storage| async move {
      let storage = C4GHStorage::new(get_decryption_keys().await, storage);
      let mut search = VcfSearch::new(Storage::new(storage));
      let query = Query::new_with_default_request("spec-v4.3", Format::Vcf);
      let response = search.search(query).await.unwrap();

      println!("{:#?}", response);

      Some(("spec-v4.3.vcf.gz.c4gh".to_string(), (response, Body).into()))
    })
    .await;
  }

  #[cfg(feature = "experimental")]
  #[tokio::test]
  async fn search_all_range_c4gh() {
    with_local_storage_c4gh(|storage| async move {
      let storage = C4GHStorage::new(get_decryption_keys().await, storage);
      let mut search = VcfSearch::new(Storage::new(storage));
      let query = Query::new_with_default_request("spec-v4.3", Format::Vcf)
        .with_reference_name("20")
        .with_start(150)
        .with_end(153);
      let response = search.search(query).await.unwrap();

      println!("{:#?}", response);

      Some(("spec-v4.3.vcf.gz.c4gh".to_string(), (response, Body).into()))
    })
    .await;
  }

  async fn test_reference_name_with_seq_range(
    storage: Storage,
  ) -> Option<(String, ConcatResponse)> {
    let mut search = VcfSearch::new(storage);
    let filename = "sample1-bcbio-cancer";
    let query = Query::new_with_default_request(filename, Format::Vcf)
      .with_reference_name("chrM")
      .with_start(151)
      .with_end(153);
    let response = search.search(query).await;
    println!("{response:#?}");

    let expected_response = Ok(expected_vcf_response(filename));
    assert_eq!(response, expected_response);

    Some((
      VCF_FILE_NAME_SAMPLE.to_string(),
      (response.unwrap(), Body).into(),
    ))
  }

  fn expected_vcf_response(filename: &str) -> Response {
    Response::new(
      Format::Vcf,
      vec![Url::new(expected_url(filename))
        .with_headers(Headers::default().with_header("Range", "bytes=0-3493"))],
    )
  }

  pub(crate) async fn with_local_storage<F, Fut>(test: F)
  where
    F: FnOnce(Storage) -> Fut,
    Fut: Future<Output = Option<(String, ConcatResponse)>>,
  {
    with_local_storage_fn(test, "data/vcf", &[]).await
  }

  pub(crate) fn expected_url(name: &str) -> String {
    format!("http://127.0.0.1:8081/{name}.vcf.gz")
  }
}
