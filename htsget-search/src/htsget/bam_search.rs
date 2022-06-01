//! Module providing the search capability using BAM/BAI files
//!
use std::sync::Arc;

use async_trait::async_trait;
use noodles::bam::bai;
use noodles::bam::bai::index::ReferenceSequence;
use noodles::bam::bai::Index;
use noodles::bgzf::VirtualPosition;
use noodles::csi::BinningIndex;
use noodles::sam::Header;
use noodles::{bgzf, sam};
use noodles_bam as bam;
use tokio::io;
use tokio::io::AsyncRead;
use tokio::io::AsyncSeek;

use crate::htsget::search::{BgzfSearch, Search, SearchReads, VirtualPositionExt};
use crate::htsget::HtsGetError;
use crate::{
  htsget::search::BlockPosition,
  htsget::{Format, Query, Result},
  storage::{BytesPosition, Storage},
};
use crate::storage::DataBlock;

type AsyncReader<ReaderType> = bam::AsyncReader<bgzf::AsyncReader<ReaderType>>;

pub(crate) struct BamSearch<S> {
  storage: Arc<S>,
}

#[async_trait]
impl<ReaderType> BlockPosition for AsyncReader<ReaderType>
where
  ReaderType: AsyncRead + AsyncSeek + Unpin + Send + Sync,
{
  async fn read_bytes(&mut self) -> Option<usize> {
    self.read_record(&mut bam::Record::default()).await.ok()
  }

  async fn seek_vpos(&mut self, pos: VirtualPosition) -> io::Result<VirtualPosition> {
    self.seek(pos).await
  }

  fn virtual_position(&self) -> VirtualPosition {
    self.virtual_position()
  }
}

#[async_trait]
impl<S, ReaderType>
  BgzfSearch<S, ReaderType, ReferenceSequence, Index, AsyncReader<ReaderType>, Header>
  for BamSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + AsyncSeek + Unpin + Send + Sync,
{
  type ReferenceSequenceHeader = sam::header::ReferenceSequence;

  fn max_seq_position(ref_seq: &Self::ReferenceSequenceHeader) -> i32 {
    ref_seq.len()
  }

  async fn get_byte_ranges_for_unmapped(
    &self,
    id: &str,
    format: &Format,
    index: &Index,
  ) -> Result<Vec<BytesPosition>> {
    let last_interval = index
      .reference_sequences()
      .iter()
      .rev()
      .find_map(|rs| rs.intervals().last().cloned());

    let start = match last_interval {
      Some(start) => start,
      None => {
        let (bam_reader, _) = self.create_reader(id, format).await?;
        bam_reader.virtual_position()
      }
    };

    let file_size = self
      .storage
      .head(format.fmt_file(id))
      .await
      .map_err(|_| HtsGetError::io_error("Reading file size"))?;

    Ok(vec![BytesPosition::default()
      .with_start(start.bytes_range_start())
      .with_end(file_size)])
  }
}

#[async_trait]
impl<S, ReaderType> Search<S, ReaderType, ReferenceSequence, Index, AsyncReader<ReaderType>, Header>
  for BamSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + AsyncSeek + Unpin + Send + Sync,
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
    query: Query,
  ) -> Result<Vec<BytesPosition>> {
    self
      .get_byte_ranges_for_reference_name_reads(&reference_name, index, query)
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
  ReaderType: AsyncRead + AsyncSeek + Unpin + Send + Sync,
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
    let start = query.start.map(|start| start as i32);
    let end = query.end.map(|end| end as i32);
    self
      .get_byte_ranges_for_reference_sequence_bgzf(query, ref_seq, ref_seq_id, index, start, end)
      .await
  }
}

impl<S, ReaderType> BamSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + AsyncSeek + Unpin + Send + Sync,
{
  pub fn new(storage: Arc<S>) -> Self {
    Self { storage }
  }
}

#[cfg(test)]
pub mod tests {
  use std::future::Future;

  use htsget_config::regex_resolver::RegexResolver;

  use crate::htsget::{Class, Headers, Response, Url};
  use crate::storage::axum_server::HttpsFormatter;
  use crate::storage::local::LocalStorage;

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
        vec![Url::new(expected_url())
          .with_headers(Headers::default().with_header("Range", "bytes=0-2596798"))],
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
            .with_headers(Headers::default().with_header("Range", "bytes=0-4667")),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=2060795-2596798")),
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
            .with_headers(Headers::default().with_header("Range", "bytes=0-4667")),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=977196-2128165")),
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
            .with_headers(Headers::default().with_header("Range", "bytes=0-4667")),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=256721-647345")),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=824361-842100")),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=977196-996014")),
        ],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
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
    F: FnOnce(Arc<LocalStorage<HttpsFormatter>>) -> Fut,
    Fut: Future<Output = ()>,
  {
    let base_path = std::env::current_dir()
      .unwrap()
      .parent()
      .unwrap()
      .join("data/bam");
    test(Arc::new(
      LocalStorage::new(
        base_path,
        RegexResolver::new(".*", "$0").unwrap(),
        HttpsFormatter::new("127.0.0.1", "8081").unwrap(),
      )
      .unwrap(),
    ))
    .await
  }

  pub(crate) fn expected_url() -> String {
    "https://127.0.0.1:8081/data/htsnexus_test_NA12878.bam".to_string()
  }
}
