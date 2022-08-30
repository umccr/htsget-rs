//! Module providing the search capability using BAM/BAI files
//!
use std::sync::Arc;

use async_trait::async_trait;
use noodles::bam::bai;
use noodles::bam::bai::index::ReferenceSequence;
use noodles::bam::bai::Index;
use noodles::bgzf::VirtualPosition;
use noodles::csi::index::reference_sequence::bin::Chunk;
use noodles::csi::BinningIndex;
use noodles::sam::Header;
use noodles::{bgzf, sam};
use noodles_bam as bam;
use tokio::io;
use tokio::io::AsyncRead;
use tracing::instrument;

use crate::htsget::search::{BgzfSearch, BinningIndexExt, Search, SearchAll, SearchReads};
use crate::htsget::Class::Body;
use crate::htsget::HtsGetError;
use crate::{
  htsget::{Format, Query, Result},
  storage::{BytesPosition, Storage},
};

type AsyncReader<ReaderType> = bam::AsyncReader<bgzf::AsyncReader<ReaderType>>;

pub(crate) struct BamSearch<S> {
  storage: Arc<S>,
}

impl BinningIndexExt for Index {
  #[instrument(level = "trace", skip_all)]
  fn get_all_chunks(&self) -> Vec<&Chunk> {
    self
      .reference_sequences()
      .iter()
      .flat_map(|ref_seq| ref_seq.bins())
      .flat_map(|bin| bin.chunks())
      .collect()
  }
}

#[async_trait]
impl<S, ReaderType>
  BgzfSearch<S, ReaderType, ReferenceSequence, Index, AsyncReader<ReaderType>, Header>
  for BamSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
{
  type ReferenceSequenceHeader = sam::header::ReferenceSequence;

  fn max_seq_position(ref_seq: &Self::ReferenceSequenceHeader) -> usize {
    ref_seq.len().get()
  }

  #[instrument(level = "trace", skip_all)]
  async fn get_byte_ranges_for_unmapped(
    &self,
    id: &str,
    format: &Format,
    index: &Index,
  ) -> Result<Vec<BytesPosition>> {
    let last_interval = index.first_record_in_last_linear_bin_start_position();
    let start = match last_interval {
      Some(start) => start,
      None => {
        VirtualPosition::try_from((self.get_header_end_offset(index).await?, 0)).map_err(|err| {
          HtsGetError::InvalidInput(format!(
            "invalid virtual position generated from header end offset `{}`.",
            err
          ))
        })?
      }
    };

    Ok(vec![BytesPosition::default()
      .with_start(start.compressed())
      .with_end(self.position_at_eof(id, format).await?)
      .with_class(Body)])
  }
}

#[async_trait]
impl<S, ReaderType> Search<S, ReaderType, ReferenceSequence, Index, AsyncReader<ReaderType>, Header>
  for BamSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
{
  fn init_reader(inner: ReaderType) -> AsyncReader<ReaderType> {
    AsyncReader::new(inner)
  }

  async fn read_raw_header(reader: &mut AsyncReader<ReaderType>) -> io::Result<String> {
    let header = reader.read_header().await;
    reader.read_reference_sequences().await?;
    header
  }

  async fn read_index_inner<T: AsyncRead + Unpin + Send>(inner: T) -> io::Result<Index> {
    let mut reader = bai::AsyncReader::new(inner);
    reader.read_header().await?;
    reader.read_index().await
  }

  async fn get_byte_ranges_for_reference_name(
    &self,
    reference_name: String,
    index: &Index,
    header: &Header,
    query: Query,
  ) -> Result<Vec<BytesPosition>> {
    self
      .get_byte_ranges_for_reference_name_reads(&reference_name, index, header, query)
      .await
  }

  fn get_storage(&self) -> Arc<S> {
    Arc::clone(&self.storage)
  }

  fn get_format(&self) -> Format {
    Format::Bam
  }
}

#[async_trait]
impl<S, ReaderType>
  SearchReads<S, ReaderType, ReferenceSequence, Index, AsyncReader<ReaderType>, Header>
  for BamSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
{
  async fn get_reference_sequence_from_name<'a>(
    &self,
    header: &'a Header,
    name: &str,
  ) -> Option<(usize, &'a String, &'a sam::header::ReferenceSequence)> {
    header.reference_sequences().get_full(name)
  }

  async fn get_byte_ranges_for_unmapped_reads(
    &self,
    query: &Query,
    bai_index: &Index,
  ) -> Result<Vec<BytesPosition>> {
    self
      .get_byte_ranges_for_unmapped(&query.id, &self.get_format(), bai_index)
      .await
  }

  async fn get_byte_ranges_for_reference_sequence(
    &self,
    ref_seq: &sam::header::ReferenceSequence,
    ref_seq_id: usize,
    query: Query,
    index: &Index,
  ) -> Result<Vec<BytesPosition>> {
    self
      .get_byte_ranges_for_reference_sequence_bgzf(query, ref_seq, ref_seq_id, index)
      .await
  }
}

impl<S, ReaderType> BamSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
{
  /// Create the bam search.
  pub fn new(storage: Arc<S>) -> Self {
    Self { storage }
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use std::future::Future;

  use htsget_test_utils::util::expected_bgzf_eof_data_url;

  use crate::htsget::from_storage::tests::{
    with_local_storage as with_local_storage_path,
    with_local_storage_tmp as with_local_storage_tmp_path,
  };
  use crate::htsget::{Class, Class::Body, Class::Header, Headers, Response, Url};
  use crate::storage::local::LocalStorage;
  use crate::storage::ticket_server::HttpTicketFormatter;

  use super::*;

  #[tokio::test]
  async fn search_all_reads() {
    with_local_storage(|storage| async move {
      let search = BamSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878", Format::Bam);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-2596770")),
          Url::new(expected_bgzf_eof_data_url()),
        ],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_unmapped_reads() {
    with_local_storage(|storage| async move {
      let search = BamSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878", Format::Bam).with_reference_name("*");
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-4667"))
            .with_class(Header),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=2060795-2596770"))
            .with_class(Body),
          Url::new(expected_bgzf_eof_data_url()).with_class(Body),
        ],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_without_seq_range() {
    with_local_storage(|storage| async move {
      let search = BamSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878", Format::Bam).with_reference_name("20");
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-4667"))
            .with_class(Header),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=977196-2128165"))
            .with_class(Body),
          Url::new(expected_bgzf_eof_data_url()).with_class(Body),
        ],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_with_seq_range() {
    with_local_storage(|storage| async move {
      let search = BamSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878", Format::Bam)
        .with_reference_name("11")
        .with_start(5015000)
        .with_end(5050000);
      let response = search.search(query).await;
      println!("{:#?}", response);

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
          Url::new(expected_bgzf_eof_data_url()).with_class(Body),
        ],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_many_response_urls() {
    with_local_storage(|storage| async move {
      let search = BamSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878", Format::Bam)
        .with_reference_name("11")
        .with_start(4999976)
        .with_end(5003981);
      let response = search.search(query).await;
      println!("{:#?}", response);

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
          Url::new(expected_bgzf_eof_data_url()),
        ],
      ));
      assert_eq!(response, expected_response)
    })
    .await
  }

  #[tokio::test]
  async fn search_no_gzi() {
    with_local_storage_tmp(|storage| async move {
      let search = BamSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878", Format::Bam)
        .with_reference_name("11")
        .with_start(5015000)
        .with_end(5050000);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-4667"))
            .with_class(Header),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=256721-1065951"))
            .with_class(Body),
          Url::new(expected_bgzf_eof_data_url()).with_class(Body),
        ],
      ));
      assert_eq!(response, expected_response)
    })
    .await
  }

  #[tokio::test]
  async fn search_header() {
    with_local_storage(|storage| async move {
      let search = BamSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878", Format::Bam).with_class(Class::Header);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![Url::new(expected_url())
          .with_headers(Headers::default().with_header("Range", "bytes=0-4667"))
          .with_class(Class::Header)],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  pub(crate) async fn with_local_storage<F, Fut>(test: F)
  where
    F: FnOnce(Arc<LocalStorage<HttpTicketFormatter>>) -> Fut,
    Fut: Future<Output = ()>,
  {
    with_local_storage_path(test, "data/bam").await
  }

  async fn with_local_storage_tmp<F, Fut>(test: F)
  where
    F: FnOnce(Arc<LocalStorage<HttpTicketFormatter>>) -> Fut,
    Fut: Future<Output = ()>,
  {
    with_local_storage_tmp_path(
      test,
      "data/bam",
      &["htsnexus_test_NA12878.bam", "htsnexus_test_NA12878.bam.bai"],
    )
    .await
  }

  pub(crate) fn expected_url() -> String {
    "http://127.0.0.1:8081/data/htsnexus_test_NA12878.bam".to_string()
  }
}
