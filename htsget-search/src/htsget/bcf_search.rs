//! Module providing the search capability using BCF files
//!

use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use futures::prelude::stream::FuturesUnordered;
use noodles::bcf;
use noodles::bcf::AsyncReader;
use noodles::bgzf::VirtualPosition;
use noodles::csi;
use noodles::csi::index::ReferenceSequence;
use noodles::csi::Index;
use noodles::vcf;
use tokio::{fs::File, io};

use crate::htsget::search::{
  find_first, AsyncHeaderResult, AsyncIndexResult, BgzfSearch, BlockPosition, Search,
};
use crate::{
  htsget::{Format, Query, Result},
  storage::{AsyncStorage, BytesRange},
};

pub(crate) struct BcfSearch<S> {
  storage: Arc<S>,
}

#[async_trait]
impl BlockPosition for bcf::AsyncReader<File> {
  async fn read_bytes(&mut self) -> Option<usize> {
    self.read_record(&mut bcf::Record::default()).await.ok()
  }

  async fn seek(&mut self, pos: VirtualPosition) -> io::Result<VirtualPosition> {
    self.seek(pos).await
  }

  fn virtual_position(&self) -> VirtualPosition {
    self.virtual_position()
  }
}

#[async_trait]
impl<S> BgzfSearch<S, ReferenceSequence, Index, bcf::AsyncReader<File>, vcf::Header>
  for BcfSearch<S>
where
  S: AsyncStorage + Send + Sync + 'static,
{
  type ReferenceSequenceHeader = PhantomData<Self>;

  fn max_seq_position(_ref_seq: &Self::ReferenceSequenceHeader) -> i32 {
    Self::MAX_SEQ_POSITION
  }
}

#[async_trait]
impl<S> Search<S, ReferenceSequence, Index, bcf::AsyncReader<File>, vcf::Header> for BcfSearch<S>
where
  S: AsyncStorage + Send + Sync + 'static,
{
  const READER_FN: fn(tokio::fs::File) -> AsyncReader<File> = bcf::AsyncReader::new;
  const HEADER_FN: fn(&'_ mut AsyncReader<File>) -> AsyncHeaderResult = |reader| {
    Box::pin(async move {
      reader.read_file_format().await?;
      reader.read_header().await
    })
  };
  const INDEX_FN: fn(PathBuf) -> AsyncIndexResult<'static, Index> =
    |path| Box::pin(async move { csi::r#async::read(path).await });

  async fn get_byte_ranges_for_reference_name(
    &self,
    key: String,
    reference_name: String,
    index: &Index,
    query: &Query,
  ) -> Result<Vec<BytesRange>> {
    let (_, header) = self.create_reader(&key).await?;

    // We are assuming the order of the contigs in the header and the references sequences
    // in the index is the same
    let futures = FuturesUnordered::new();
    for (ref_seq_index, (name, contig)) in header.contigs().iter().enumerate() {
      let owned_contig = contig.clone();
      let owned_name = name.to_owned();
      let owned_reference_name = reference_name.clone();
      futures.push(tokio::spawn(async move {
        if owned_name == owned_reference_name {
          Some((ref_seq_index, (owned_name, owned_contig)))
        } else {
          None
        }
      }));
    }
    let (ref_seq_index, (_, contig)) = find_first(
      &format!("Reference name not found in the header: {}", reference_name,),
      futures,
    )
    .await?;
    let maybe_len = contig.len();

    let seq_start = query.start.map(|start| start as i32);
    let seq_end = query.end.map(|end| end as i32).or(maybe_len);
    let byte_ranges = self
      .get_byte_ranges_for_reference_sequence_bgzf(
        key,
        &PhantomData,
        ref_seq_index,
        index,
        seq_start,
        seq_end,
      )
      .await?;
    Ok(byte_ranges)
  }

  fn get_keys_from_id(&self, id: &str) -> (String, String) {
    let bcf_key = format!("{}.bcf", id);
    let csi_key = format!("{}.bcf.csi", id);
    (bcf_key, csi_key)
  }

  fn get_storage(&self) -> Arc<S> {
    self.storage.clone()
  }

  fn get_format(&self) -> Format {
    Format::Bcf
  }
}

impl<S> BcfSearch<S>
where
  S: AsyncStorage + Send + Sync + 'static,
{
  const MAX_SEQ_POSITION: i32 = (1 << 29) - 1; // see https://github.com/zaeleus/noodles/issues/25#issuecomment-868871298

  pub fn new(storage: Arc<S>) -> Self {
    Self { storage }
  }
}

#[cfg(test)]
pub mod tests {
  use std::future::Future;

  use crate::htsget::{Class, Headers, HtsGetError, Response, Url};
  use crate::storage::blocking::local::LocalStorage;
  use htsget_id_resolver::RegexResolver;

  use super::*;

  #[tokio::test]
  async fn search_all_variants() {
    with_local_storage(|storage| async move {
      let search = BcfSearch::new(storage.clone());
      let filename = "sample1-bcbio-cancer";
      let query = Query::new(filename);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bcf,
        vec![Url::new(expected_url(storage, filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-3530"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await
  }

  #[tokio::test]
  async fn search_reference_name_without_seq_range() {
    with_local_storage(|storage| async move {
      let search = BcfSearch::new(storage.clone());
      let filename = "vcf-spec-v4.3";
      let query = Query::new(filename).with_reference_name("20");
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bcf,
        vec![Url::new(expected_url(storage, filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-950"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await
  }

  #[tokio::test]
  async fn search_reference_name_with_seq_range() {
    with_local_storage(|storage| async move {
      let search = BcfSearch::new(storage.clone());
      let filename = "sample1-bcbio-cancer";
      let query = Query::new(filename)
        .with_reference_name("chrM")
        .with_start(151)
        .with_end(153);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bcf,
        vec![Url::new(expected_url(storage, filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-3530"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await
  }

  #[tokio::test]
  async fn search_reference_name_with_invalid_seq_range() {
    with_local_storage(|storage| async move {
      let search = BcfSearch::new(storage);
      let filename = "sample1-bcbio-cancer";
      let query = Query::new(filename)
        .with_reference_name("chrM")
        .with_start(0)
        .with_end(153);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Err(HtsGetError::InvalidRange("0-153".to_string()));
      assert_eq!(response, expected_response)
    })
    .await
  }

  #[tokio::test]
  async fn search_header() {
    with_local_storage(|storage| async move {
      let search = BcfSearch::new(storage.clone());
      let filename = "vcf-spec-v4.3";
      let query = Query::new(filename).with_class(Class::Header);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bcf,
        vec![Url::new(expected_url(storage, filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-950"))
          .with_class(Class::Header)],
      ));
      assert_eq!(response, expected_response)
    })
    .await
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
      .join("data/bcf");

    test(Arc::new(
      LocalStorage::new(base_path, RegexResolver::new(".*", "$0").unwrap()).unwrap(),
    ))
    .await
  }

  pub(crate) fn expected_url(storage: Arc<LocalStorage>, name: &str) -> String {
    format!(
      "file://{}",
      storage
        .base_path()
        .join(format!("{}.bcf", name))
        .to_string_lossy()
    )
  }
}
