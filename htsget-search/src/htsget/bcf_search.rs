//! Module providing the search capability using BCF files
//!

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::stream::FuturesOrdered;
use noodles::bcf;
use noodles::bgzf::VirtualPosition;
use noodles::csi::binning_index::index::reference_sequence::index::BinnedIndex;
use noodles::csi::binning_index::index::ReferenceSequence;
use noodles::csi::Index;
use noodles::vcf::Header;
use noodles::{bgzf, csi};
use tokio::io;
use tokio::io::AsyncRead;
use tracing::{instrument, trace};

use crate::htsget::search::{find_first, BgzfSearch, Search};
use crate::htsget::ParsedHeader;
use crate::storage::{BytesPosition, Storage};
use crate::{Format, Query, Result};

type AsyncReader<ReaderType> = bcf::AsyncReader<bgzf::AsyncReader<ReaderType>>;

/// Allows searching through bcf files.
pub struct BcfSearch<S> {
  storage: Arc<S>,
}

#[async_trait]
impl<S, ReaderType> BgzfSearch<S, BinnedIndex, ReaderType, AsyncReader<ReaderType>, Header>
  for BcfSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
{
  async fn read_bytes(_header: &Header, reader: &mut AsyncReader<ReaderType>) -> Option<usize> {
    reader.read_lazy_record(&mut Default::default()).await.ok()
  }

  fn virtual_position(&self, reader: &AsyncReader<ReaderType>) -> VirtualPosition {
    reader.virtual_position()
  }
}

#[async_trait]
impl<S, ReaderType>
  Search<S, ReaderType, ReferenceSequence<BinnedIndex>, Index, AsyncReader<ReaderType>, Header>
  for BcfSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
{
  fn init_reader(inner: ReaderType) -> AsyncReader<ReaderType> {
    AsyncReader::new(inner)
  }

  async fn read_header(reader: &mut AsyncReader<ReaderType>) -> io::Result<Header> {
    reader.read_file_format().await?;

    Ok(
      reader
        .read_header()
        .await?
        .parse::<ParsedHeader<Header>>()?
        .into_inner(),
    )
  }

  async fn read_index_inner<T: AsyncRead + Unpin + Send>(inner: T) -> io::Result<Index> {
    csi::AsyncReader::new(inner).read_index().await
  }

  #[instrument(level = "trace", skip(self, index, header, query))]
  async fn get_byte_ranges_for_reference_name(
    &self,
    reference_name: String,
    index: &Index,
    header: &Header,
    query: &Query,
  ) -> Result<Vec<BytesPosition>> {
    trace!("getting byte ranges for reference name");
    // We are assuming the order of the contigs in the header and the references sequences
    // in the index is the same
    let mut futures = FuturesOrdered::new();
    for (ref_seq_index, (name, _)) in header.contigs().iter().enumerate() {
      let owned_name = name.to_owned();
      let owned_reference_name = reference_name.clone();
      futures.push_back(tokio::spawn(async move {
        if owned_name == owned_reference_name {
          Some((ref_seq_index, owned_name))
        } else {
          None
        }
      }));
    }
    let (ref_seq_id, _) = find_first(
      &format!("reference name not found in header: {reference_name}"),
      futures,
    )
    .await?;

    let byte_ranges = self
      .get_byte_ranges_for_reference_sequence_bgzf(query, ref_seq_id, index)
      .await?;
    Ok(byte_ranges)
  }

  fn get_storage(&self) -> Arc<S> {
    self.storage.clone()
  }

  fn get_format(&self) -> Format {
    Format::Bcf
  }
}

impl<S, ReaderType> BcfSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
{
  /// Create the bcf search.
  pub fn new(storage: Arc<S>) -> Self {
    Self { storage }
  }
}

#[cfg(test)]
mod tests {
  use std::future::Future;

  use htsget_config::storage::local::LocalStorage as ConfigLocalStorage;
  use htsget_test::http::concat::ConcatResponse;
  use htsget_test::util::expected_bgzf_eof_data_url;

  #[cfg(feature = "s3-storage")]
  use crate::htsget::from_storage::tests::with_aws_storage_fn;
  use crate::htsget::from_storage::tests::with_local_storage_fn;
  use crate::htsget::search::SearchAll;
  use crate::storage::local::LocalStorage;
  use crate::{Class::Header, Headers, HtsGetError::NotFound, Response, Url};

  use super::*;

  const DATA_LOCATION: &str = "data/bcf";
  const INDEX_FILE_LOCATION: &str = "vcf-spec-v4.3.bcf.csi";
  const BCF_FILE_NAME_SPEC: &str = "spec-v4.3.bcf";
  const BCF_FILE_NAME_SAMPLE: &str = "sample1-bcbio-cancer.bcf";

  #[tokio::test]
  async fn search_all_variants() {
    with_local_storage(|storage| async move {
      let search = BcfSearch::new(storage.clone());
      let filename = "sample1-bcbio-cancer";
      let query = Query::new_with_default_request(filename, Format::Bcf);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(expected_bcf_response(filename));
      assert_eq!(response, expected_response);

      Some((BCF_FILE_NAME_SAMPLE.to_string(), response.unwrap().into()))
    })
    .await
  }

  #[tokio::test]
  async fn search_reference_name_without_seq_range() {
    with_local_storage(|storage| async move {
      let search = BcfSearch::new(storage.clone());
      let filename = "vcf-spec-v4.3";
      let query = Query::new_with_default_request(filename, Format::Bcf).with_reference_name("20");
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Bcf,
        vec![
          Url::new(expected_url(filename))
            .with_headers(Headers::default().with_header("Range", "bytes=0-949")),
          Url::new(expected_bgzf_eof_data_url()),
        ],
      ));
      assert_eq!(response, expected_response);

      Some((BCF_FILE_NAME_SPEC.to_string(), response.unwrap().into()))
    })
    .await
  }

  #[tokio::test]
  async fn search_reference_name_with_seq_range() {
    with_local_storage(
      |storage| async move { test_reference_sequence_with_seq_range(storage).await },
    )
    .await
  }

  #[tokio::test]
  async fn search_reference_name_no_end_position() {
    with_local_storage(|storage| async move {
      let search = BcfSearch::new(storage.clone());
      let filename = "sample1-bcbio-cancer";
      let query = Query::new_with_default_request(filename, Format::Bcf)
        .with_reference_name("chrM")
        .with_start(151);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(expected_bcf_response(filename));
      assert_eq!(response, expected_response);

      Some((BCF_FILE_NAME_SAMPLE.to_string(), response.unwrap().into()))
    })
    .await
  }

  #[tokio::test]
  async fn search_no_gzi() {
    with_local_storage_fn(
      |storage| async move { test_reference_sequence_with_seq_range(storage).await },
      DATA_LOCATION,
      &["sample1-bcbio-cancer.bcf", "sample1-bcbio-cancer.bcf.csi"],
    )
    .await
  }

  #[tokio::test]
  async fn search_header() {
    with_local_storage(|storage| async move {
      let search = BcfSearch::new(storage.clone());
      let filename = "vcf-spec-v4.3";
      let query = Query::new_with_default_request(filename, Format::Bcf).with_class(Header);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Bcf,
        vec![Url::new(expected_url(filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-949"))
          .with_class(Header)],
      ));
      assert_eq!(response, expected_response);

      Some((BCF_FILE_NAME_SPEC.to_string(), response.unwrap().into()))
    })
    .await
  }

  #[tokio::test]
  async fn search_non_existent_id_reference_name() {
    with_local_storage_fn(
      |storage| async move {
        let search = BcfSearch::new(storage.clone());
        let query = Query::new_with_default_request("vcf-spec-v4.3", Format::Bcf);
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
        let search = BcfSearch::new(storage.clone());
        let query =
          Query::new_with_default_request("vcf-spec-v4.3", Format::Bcf).with_reference_name("chrM");
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
        let search = BcfSearch::new(storage.clone());
        let query =
          Query::new_with_default_request("vcf-spec-v4.3", Format::Bcf).with_class(Header);
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
  async fn search_header_with_non_existent_reference_name() {
    with_local_storage(|storage| async move {
      let search = BcfSearch::new(storage.clone());
      let query =
        Query::new_with_default_request("vcf-spec-v4.3", Format::Bcf).with_reference_name("chr1");
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
        let search = BcfSearch::new(storage.clone());
        let query =
          Query::new_with_default_request("vcf-spec-v4.3", Format::Bcf).with_class(Header);

        let index = search.read_index(&query).await.unwrap();
        let response = search.get_header_end_offset(&index).await;

        assert_eq!(response, Ok(65536));

        None
      },
      DATA_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[cfg(feature = "s3-storage")]
  #[tokio::test]
  async fn search_non_existent_id_reference_name_aws() {
    with_aws_storage_fn(
      |storage| async move {
        let search = BcfSearch::new(storage);
        let query = Query::new_with_default_request("vcf-spec-v4.3", Format::Bcf);
        let response = search.search(query).await;
        assert!(response.is_err());

        None
      },
      DATA_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[cfg(feature = "s3-storage")]
  #[tokio::test]
  async fn search_non_existent_id_all_reads_aws() {
    with_aws_storage_fn(
      |storage| async move {
        let search = BcfSearch::new(storage);
        let query =
          Query::new_with_default_request("vcf-spec-v4.3", Format::Bcf).with_reference_name("chrM");
        let response = search.search(query).await;
        assert!(response.is_err());

        None
      },
      DATA_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  #[cfg(feature = "s3-storage")]
  #[tokio::test]
  async fn search_non_existent_id_header_aws() {
    with_aws_storage_fn(
      |storage| async move {
        let search = BcfSearch::new(storage);
        let query =
          Query::new_with_default_request("vcf-spec-v4.3", Format::Bcf).with_class(Header);
        let response = search.search(query).await;
        assert!(response.is_err());

        None
      },
      DATA_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  async fn test_reference_sequence_with_seq_range(
    storage: Arc<LocalStorage<ConfigLocalStorage>>,
  ) -> Option<(String, ConcatResponse)> {
    let search = BcfSearch::new(storage.clone());
    let filename = "sample1-bcbio-cancer";
    let query = Query::new_with_default_request(filename, Format::Bcf)
      .with_reference_name("chrM")
      .with_start(151)
      .with_end(153);
    let response = search.search(query).await;
    println!("{response:#?}");

    let expected_response = Ok(expected_bcf_response(filename));
    assert_eq!(response, expected_response);

    Some((BCF_FILE_NAME_SAMPLE.to_string(), response.unwrap().into()))
  }

  fn expected_bcf_response(filename: &str) -> Response {
    Response::new(
      Format::Bcf,
      vec![
        Url::new(expected_url(filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-3529")),
        Url::new(expected_bgzf_eof_data_url()),
      ],
    )
  }

  async fn with_local_storage<F, Fut>(test: F)
  where
    F: FnOnce(Arc<LocalStorage<ConfigLocalStorage>>) -> Fut,
    Fut: Future<Output = Option<(String, ConcatResponse)>>,
  {
    with_local_storage_fn(test, "data/bcf", &[]).await
  }

  fn expected_url(name: &str) -> String {
    format!("http://127.0.0.1:8081/data/{name}.bcf")
  }
}
