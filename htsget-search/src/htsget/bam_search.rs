//! Module providing the search capability using BAM/BAI files
//!
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use futures::AsyncRead;
use noodles::bam;
use noodles::bam::bai::index::ReferenceSequence;
use noodles::bam::bai::Index;
use noodles::bam::{bai, AsyncReader};
use noodles::bgzf::VirtualPosition;
use noodles::csi::BinningIndex;
use noodles::sam;
use noodles::sam::Header;
use tokio::fs::File;

use crate::htsget::search::{AsyncHeaderResult, AsyncIndexResult, BgzfSearch, Search, SearchReads};
use crate::htsget::HtsGetError;
use crate::{
  htsget::search::{BlockPosition, VirtualPositionExt},
  htsget::{Format, Query, Result},
  storage::{AsyncStorage, BytesRange},
};

pub(crate) struct BamSearch<S> {
  storage: Arc<S>,
}

#[async_trait]
impl BlockPosition for bam::AsyncReader<File> {
  async fn read_bytes(&mut self) -> Option<usize> {
    self.read_record(&mut bam::Record::default()).await.ok()
  }

  async fn seek(&mut self, pos: VirtualPosition) -> std::io::Result<VirtualPosition> {
    self.seek(pos).await
  }

  fn virtual_position(&self) -> VirtualPosition {
    self.virtual_position()
  }
}

#[async_trait]
impl<S> BgzfSearch<S, ReferenceSequence, bai::Index, bam::AsyncReader<File>, sam::Header>
  for BamSearch<S>
where
  S: AsyncStorage + Send + Sync + 'static,
{
  type ReferenceSequenceHeader = sam::header::ReferenceSequence;

  fn max_seq_position(ref_seq: &Self::ReferenceSequenceHeader) -> i32 {
    ref_seq.len()
  }

  async fn get_byte_ranges_for_unmapped(
    &self,
    key: &str,
    index: &bai::Index,
  ) -> Result<Vec<BytesRange>> {
    let last_interval = index
      .reference_sequences()
      .iter()
      .rev()
      .find_map(|rs| rs.intervals().last().cloned());

    let start = match last_interval {
      Some(start) => start,
      None => {
        let (bam_reader, _) = self.create_reader(key).await?;
        bam_reader.virtual_position()
      }
    };

    let file_size = self
      .storage
      .head(key)
      .await
      .map_err(|_| HtsGetError::io_error("Reading file size"))?;
    Ok(vec![BytesRange::default()
      .with_start(start.bytes_range_start())
      .with_end(file_size)])
  }
}

#[async_trait]
impl<S> Search<S, ReferenceSequence, bai::Index, bam::AsyncReader<File>, sam::Header>
  for BamSearch<S>
where
  S: AsyncStorage + Send + Sync + 'static,
{
  const READER_FN: fn(File) -> AsyncReader<File> = bam::AsyncReader::new;
  const HEADER_FN: fn(&'_ mut AsyncReader<File>) -> AsyncHeaderResult = |reader| {
    Box::pin(async move {
      let header = reader.read_header().await;
      reader.read_reference_sequences().await?;
      header
    })
  };
  const INDEX_FN: fn(PathBuf, bytes::Bytes) -> AsyncIndexResult<'static, Index> =
    |path, stream| Box::pin(async move { bai::r#async::read(path).await });

  async fn get_byte_ranges_for_reference_name(
    &self,
    key: String,
    reference_name: String,
    index: &Index,
    query: &Query,
  ) -> Result<Vec<BytesRange>> {
    self
      .get_byte_ranges_for_reference_name_reads(key, &reference_name, index, query)
      .await
  }

  fn get_keys_from_id(&self, id: &str) -> (String, String) {
    let bam_key = format!("{}.bam", id);
    let bai_key = format!("{}.bai", bam_key);
    (bam_key, bai_key)
  }

  fn get_storage(&self) -> Arc<S> {
    Arc::clone(&self.storage)
  }

  fn get_format(&self) -> Format {
    Format::Bam
  }
}

#[async_trait]
impl<S> SearchReads<S, ReferenceSequence, bai::Index, bam::AsyncReader<File>, sam::Header>
  for BamSearch<S>
where
  S: AsyncStorage + Send + Sync + 'static,
{
  async fn get_reference_sequence_from_name<'b>(
    &self,
    header: &'b Header,
    name: &str,
  ) -> Option<(usize, &'b String, &'b sam::header::ReferenceSequence)> {
    header.reference_sequences().get_full(name)
  }

  async fn get_byte_ranges_for_unmapped_reads(
    &self,
    bam_key: &str,
    bai_index: &Index,
  ) -> Result<Vec<BytesRange>> {
    self.get_byte_ranges_for_unmapped(bam_key, bai_index).await
  }

  async fn get_byte_ranges_for_reference_sequence(
    &self,
    key: String,
    ref_seq: &sam::header::ReferenceSequence,
    ref_seq_id: usize,
    query: &Query,
    index: &Index,
  ) -> Result<Vec<BytesRange>> {
    self
      .get_byte_ranges_for_reference_sequence_bgzf(
        key,
        ref_seq,
        ref_seq_id,
        index,
        query.start.map(|start| start as i32),
        query.end.map(|end| end as i32),
      )
      .await
  }
}

impl<S> BamSearch<S>
where
  S: AsyncStorage + Send + Sync + 'static,
{
  pub fn new(storage: Arc<S>) -> Self {
    Self { storage }
  }
}

#[cfg(test)]
pub mod tests {
  use std::future::Future;

  use crate::htsget::{Class, Headers, Response, Url};
  use crate::storage::blocking::local::LocalStorage;
  use htsget_id_resolver::RegexResolver;

  use super::*;

  #[tokio::test]
  async fn search_all_reads() {
    with_local_storage(|storage| async move {
      let search = BamSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878");
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![Url::new(expected_url(storage))
          .with_headers(Headers::default().with_header("Range", "bytes=4668-2596799"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_unmapped_reads() {
    with_local_storage(|storage| async move {
      let search = BamSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878").with_reference_name("*");
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![Url::new(expected_url(storage))
          .with_headers(Headers::default().with_header("Range", "bytes=2060795-2596799"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_without_seq_range() {
    with_local_storage(|storage| async move {
      let search = BamSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878").with_reference_name("20");
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![Url::new(expected_url(storage))
          .with_headers(Headers::default().with_header("Range", "bytes=977196-2128166"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_with_seq_range() {
    with_local_storage(|storage| async move {
      let search = BamSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878")
        .with_reference_name("11")
        .with_start(5015000)
        .with_end(5050000);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url(storage.clone()))
            .with_headers(Headers::default().with_header("Range", "bytes=256721-647346")),
          Url::new(expected_url(storage.clone()))
            .with_headers(Headers::default().with_header("Range", "bytes=824361-842101")),
          Url::new(expected_url(storage))
            .with_headers(Headers::default().with_header("Range", "bytes=977196-996015")),
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
      let query = Query::new("htsnexus_test_NA12878").with_class(Class::Header);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![Url::new(expected_url(storage))
          .with_headers(Headers::default().with_header("Range", "bytes=0-4668"))
          .with_class(Class::Header)],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  pub(crate) async fn with_local_storage<F, Fut>(test: F)
  where
    F: FnOnce(Arc<LocalStorage>) -> Fut,
    Fut: Future<Output = ()>,
  {
    let base_path = std::env::current_dir()
      .unwrap()
      .parent()
      .unwrap()
      .join("data/bam");
    test(Arc::new(
      LocalStorage::new(base_path, RegexResolver::new(".*", "$0").unwrap()).unwrap(),
    ))
    .await
  }

  pub(crate) fn expected_url(storage: Arc<LocalStorage>) -> String {
    format!(
      "file://{}",
      storage
        .base_path()
        .join("htsnexus_test_NA12878.bam")
        .to_string_lossy()
    )
  }
}
