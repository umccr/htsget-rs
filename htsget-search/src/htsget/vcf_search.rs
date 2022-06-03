//! Module providing the search capability using VCF files
//!

use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;
use futures::prelude::stream::FuturesUnordered;
use noodles::bgzf;
use noodles::bgzf::VirtualPosition;
use noodles::tabix;
use noodles::tabix::index::ReferenceSequence;
use noodles::tabix::Index;
use noodles::vcf::Header;
use noodles_vcf as vcf;
use tokio::io;
use tokio::io::AsyncRead;
use tokio::io::AsyncSeek;

use crate::htsget::search::{find_first, BgzfSearch, BlockPosition, Search};
use crate::{
  htsget::{Format, Query, Result},
  storage::{BytesRange, Storage},
};

type AsyncReader<ReaderType> = vcf::AsyncReader<bgzf::AsyncReader<ReaderType>>;

pub(crate) struct VcfSearch<S> {
  storage: Arc<S>,
}

#[async_trait]
impl<ReaderType> BlockPosition for AsyncReader<ReaderType>
where
  ReaderType: AsyncRead + AsyncSeek + Unpin + Send + Sync,
{
  async fn read_bytes(&mut self) -> Option<usize> {
    self.read_record(&mut String::new()).await.ok()
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
  for VcfSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + AsyncSeek + Unpin + Send + Sync,
{
  type ReferenceSequenceHeader = PhantomData<Self>;

  fn max_seq_position(_ref_seq: &Self::ReferenceSequenceHeader) -> i32 {
    Self::MAX_SEQ_POSITION
  }
}

#[async_trait]
impl<S, ReaderType> Search<S, ReaderType, ReferenceSequence, Index, AsyncReader<ReaderType>, Header>
  for VcfSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + AsyncSeek + Unpin + Send + Sync,
{
  fn init_reader(inner: ReaderType) -> AsyncReader<ReaderType> {
    AsyncReader::new(bgzf::AsyncReader::new(inner))
  }

  async fn read_raw_header(reader: &mut AsyncReader<ReaderType>) -> io::Result<String> {
    reader.read_header().await
  }

  async fn read_index_inner<T: AsyncRead + Unpin + Send>(inner: T) -> io::Result<Index> {
    tabix::AsyncReader::new(inner).read_index().await
  }

  async fn get_byte_ranges_for_reference_name(
    &self,
    reference_name: String,
    index: &Index,
    query: Query,
  ) -> Result<Vec<BytesRange>> {
    let (_, vcf_header) = self.create_reader(&query.id, &self.get_format()).await?;
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
        query,
        &PhantomData,
        ref_seq_index,
        index,
        seq_start,
        seq_end,
      )
      .await?;
    Ok(byte_ranges)
  }

  fn get_storage(&self) -> Arc<S> {
    self.storage.clone()
  }

  fn get_format(&self) -> Format {
    Format::Vcf
  }
}

impl<S, ReaderType> VcfSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + AsyncSeek + Unpin + Send + Sync,
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

  use htsget_config::regex_resolver::RegexResolver;

  use crate::htsget::{Class, Headers, HtsGetError, Response, Url};
  use crate::storage::axum_server::HttpsFormatter;
  use crate::storage::local::LocalStorage;

  use super::*;

  #[tokio::test]
  async fn search_all_variants() {
    with_local_storage(|storage| async move {
      let search = VcfSearch::new(storage.clone());
      let filename = "sample1-bcbio-cancer";
      let query = Query::new(filename, Format::Vcf);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(expected_url(filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-3366"))],
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
      let query = Query::new(filename, Format::Vcf).with_reference_name("20");
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(expected_url(filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-822"))],
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
      let query = Query::new(filename, Format::Vcf)
        .with_reference_name("chrM")
        .with_start(151)
        .with_end(153);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(expected_url(filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-3366"))],
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
      let query = Query::new(filename, Format::Vcf)
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
      let query = Query::new(filename, Format::Vcf).with_class(Class::Header);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(expected_url(filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-822"))
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
      .join("data/vcf");
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

  pub(crate) fn expected_url(name: &str) -> String {
    format!("https://127.0.0.1:8081/data/{}.vcf.gz", name)
  }
}
