//! Module providing the search capability using VCF files
//!

use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;
use futures::prelude::stream::FuturesUnordered;
use noodles::bgzf;
use noodles::bgzf::VirtualPosition;
use noodles_vcf::{AsyncReader};
use noodles::tabix;
use noodles::tabix::index::ReferenceSequence;
use noodles::tabix::Index;
use noodles::vcf;
use noodles::vcf::Header;
use tokio::fs::File;
use tokio::io::AsyncRead;

use crate::htsget::search::{
  find_first, BgzfSearch, BlockPosition, Search,
};
use crate::{
  htsget::{Format, Query, Result},
  storage::{AsyncStorage, BytesRange},
};
use crate::htsget::HtsGetError;

pub(crate) struct VcfSearch<S> {
  storage: Arc<S>
}

#[async_trait]
impl<R> BlockPosition for AsyncReader<bgzf::AsyncReader<R>>
  where
    R: AsyncRead + Send + Sync + Unpin {
  async fn read_bytes(&mut self) -> Option<usize> {
    self.read_record(&mut String::new()).await.ok()
  }

  async fn seek(&mut self, pos: VirtualPosition) -> std::io::Result<VirtualPosition> {
    self.seek(pos).await
  }

  fn virtual_position(&self) -> VirtualPosition {
    self.virtual_position()
  }
}

#[async_trait]
impl<'a, S, R>
  BgzfSearch<'a, S, R, ReferenceSequence, tabix::Index, AsyncReader<bgzf::AsyncReader<R>>, Header>
  for VcfSearch<S>
where
  S: AsyncStorage<Streamable = R> + Send + Sync + 'static,
  R: AsyncRead + Unpin + Send + Sync
{
  type ReferenceSequenceHeader = PhantomData<Self>;

  fn max_seq_position(_ref_seq: &Self::ReferenceSequenceHeader) -> i32 {
    Self::MAX_SEQ_POSITION
  }
}

#[async_trait]
impl<'a, S, R>
  Search<'a, S, R, ReferenceSequence, tabix::Index, AsyncReader<bgzf::AsyncReader<R>>, Header>
  for VcfSearch<S>
where
  S: AsyncStorage<Streamable = R> + Send + Sync + 'static,
  R: AsyncRead + Send + Sync + Unpin
{
  fn init_reader(inner: R) -> AsyncReader<bgzf::AsyncReader<R>> {
    AsyncReader::new(bgzf::AsyncReader::new(inner))
  }

  async fn read_raw_header(reader: &mut AsyncReader<bgzf::AsyncReader<R>>) -> Result<String> {
    reader.read_header().await.map_err(|err| HtsGetError::io_error(format!("Io Error when reading vcf header: {}", err)))
  }

  async fn read_index_inner<T: AsyncRead + Unpin + Send>(inner: T) -> Result<Index> {
    let mut reader = tabix::AsyncReader::new(inner);
    reader.read_index().await.map_err(|err| HtsGetError::io_error(format!("Io Error when reading tabix index: {}", err)))
  }

  async fn get_byte_ranges_for_reference_name(
    &self,
    key: String,
    reference_name: String,
    index: &Index,
    query: &Query,
  ) -> Result<Vec<BytesRange>> {
    let (_, vcf_header) = self.create_reader(&key).await?;
    let maybe_len = vcf_header
      .contigs()
      .get(&reference_name)
      .and_then(|contig| contig.len());

    // We are assuming the order of the names and the references sequences
    // in the index is the same
    let futures = FuturesUnordered::new();
    for (index, name) in index.reference_sequence_names().iter().enumerate() {
      let owned_name = name.to_owned();
      let owned_reference_name = reference_name.clone();
      futures.push(tokio::spawn(async move {
        if owned_name == owned_reference_name {
          Some(index)
        } else {
          None
        }
      }));
    }
    let ref_seq_index = find_first(
      &format!(
        "Reference name not found in the TBI file: {}",
        reference_name,
      ),
      futures,
    )
    .await?;

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
    let vcf_key = format!("{}.vcf.gz", id);
    let tbi_key = format!("{}.vcf.gz.tbi", id);
    (vcf_key, tbi_key)
  }

  fn get_storage(&self) -> Arc<S> {
    self.storage.clone()
  }

  fn get_format(&self) -> Format {
    Format::Vcf
  }
}

impl<S, R> VcfSearch<S>
where
  S: AsyncStorage<Streamable = R> + Send + Sync + 'static,
  R: AsyncRead + Send + Sync + Unpin
{
  // 1-based
  const MAX_SEQ_POSITION: i32 = (1 << 29) - 1; // see https://github.com/zaeleus/noodles/issues/25#issuecomment-868871298

  pub fn new(storage: Arc<S>) -> Self {
    Self { storage }
  }
}

#[cfg(test)]
pub mod tests {
  use std::future::Future;

  use crate::htsget::{Class, Headers, HtsGetError, Response, Url};
  use crate::storage::local::LocalStorage;
  use htsget_id_resolver::RegexResolver;

  use super::*;

  #[tokio::test]
  async fn search_all_variants() {
    with_local_storage(|storage| async move {
      let search = VcfSearch::new(storage.clone());
      let filename = "sample1-bcbio-cancer";
      let query = Query::new(filename);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(expected_url(storage, filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-3367"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_without_seq_range() {
    with_local_storage(|storage| async move {
      let search = VcfSearch::new(storage.clone());
      let filename = "spec-v4.3";
      let query = Query::new(filename).with_reference_name("20");
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(expected_url(storage, filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-823"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_with_seq_range() {
    with_local_storage(|storage| async move {
      let search = VcfSearch::new(storage.clone());
      let filename = "sample1-bcbio-cancer";
      let query = Query::new(filename)
        .with_reference_name("chrM")
        .with_start(151)
        .with_end(153);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(expected_url(storage, filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-3367"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_with_invalid_seq_range() {
    with_local_storage(|storage| async move {
      let search = VcfSearch::new(storage);
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
    .await;
  }

  #[tokio::test]
  async fn search_header() {
    with_local_storage(|storage| async move {
      let search = VcfSearch::new(storage.clone());
      let filename = "spec-v4.3";
      let query = Query::new(filename).with_class(Class::Header);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(expected_url(storage, filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-823"))
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
      .join("data/vcf");
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
        .join(format!("{}.vcf.gz", name))
        .to_string_lossy()
    )
  }
}
