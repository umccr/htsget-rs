//! This module provides search capabilities for CRAM files.
//!

use std::convert::TryFrom;
use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;
use futures::prelude::stream::FuturesUnordered;
use futures::StreamExt;
use noodles::bam::record::ReferenceSequenceId;
use noodles::bgzf::VirtualPosition;
use noodles::cram;
use noodles::cram::Reader;
use noodles_cram::AsyncReader;
use noodles::cram::crai;
use noodles::cram::crai::{Index, Record};
use noodles::sam;
use noodles::sam::Header;
use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncSeek};
use tokio::{io, select};

use crate::htsget::search::{BlockPosition, Search, SearchAll, SearchReads};
use crate::htsget::{Format, HtsGetError, Query, Result};
use crate::storage::{AsyncStorage, BytesRange};

pub(crate) struct CramSearch<S> {
  storage: Arc<S>
}

#[async_trait]
impl<S, R> SearchAll<S, R, PhantomData<Self>, Index, AsyncReader<R>, Header> for CramSearch<S>
where
  S: AsyncStorage<Streamable = R> + Send + Sync + 'static,
  R: AsyncRead + AsyncSeek + Send + Sync + Unpin
{
  async fn get_byte_ranges_for_all(&self, key: String, index: &Index) -> Result<Vec<BytesRange>> {
    Self::bytes_ranges_from_index(
      self,
      &key,
      None,
      None,
      None,
      index,
      Arc::new(|_: &Record| true),
    )
    .await
  }

  async fn get_byte_ranges_for_header(&self, key: &str) -> Result<Vec<BytesRange>> {
    let (mut reader, _) = self.create_reader(key).await?;
    Ok(vec![BytesRange::default()
      .with_start(Self::FILE_DEFINITION_LENGTH)
      .with_end(reader.position().await?)])
  }
}

#[async_trait]
impl<'a, S, R> SearchReads<'a, S, R, PhantomData<Self>, Index, AsyncReader<R>, Header> for CramSearch<S>
where
  S: AsyncStorage<Streamable = R> + Send + Sync + 'static,
  R: AsyncRead + AsyncSeek + Send + Sync + Unpin
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
    key: &str,
    index: &Index,
  ) -> Result<Vec<BytesRange>> {
    Self::bytes_ranges_from_index(
      self,
      key,
      None,
      None,
      None,
      index,
      Arc::new(|record: &Record| record.reference_sequence_id().is_none()),
    )
    .await
  }

  async fn get_byte_ranges_for_reference_sequence(
    &self,
    key: String,
    ref_seq: &sam::header::ReferenceSequence,
    ref_seq_id: usize,
    query: &Query,
    index: &Index,
  ) -> Result<Vec<BytesRange>> {
    let ref_seq_id = ReferenceSequenceId::try_from(ref_seq_id as i32)
      .map_err(|_| HtsGetError::invalid_input("Invalid reference sequence id"))?;
    Self::bytes_ranges_from_index(
      self,
      &key,
      Some(ref_seq),
      query.start.map(|start| start as i32),
      query.end.map(|end| end as i32),
      index,
      Arc::new(move |record: &Record| record.reference_sequence_id() == Some(ref_seq_id)),
    )
    .await
  }
}

/// PhantomData is used here because of a lack of reference sequence data for CRAM.
#[async_trait]
impl<'a, S, R> Search<'a, S, R, PhantomData<Self>, Index, AsyncReader<R>, Header> for CramSearch<S>
where
  S: AsyncStorage<Streamable = R> + Send + Sync + 'static,
  R: AsyncRead + AsyncSeek + Unpin + Send + Sync
{
  fn init_reader(inner: R) -> AsyncReader<R> {
    AsyncReader::new(inner)
  }

  async fn read_raw_header(reader: &mut AsyncReader<R>) -> io::Result<String> {
    reader.read_file_definition().await?;
    reader.read_file_header().await
  }

  async fn read_index_inner<T: AsyncRead + Send + Unpin>(inner: T) -> io::Result<Index> {
    crai::AsyncReader::new(inner).read_index().await
  }

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
    let cram_key = format!("{}.cram", id);
    let crai_key = format!("{}.crai", cram_key);
    (cram_key, crai_key)
  }

  fn get_storage(&self) -> Arc<S> {
    self.storage.clone()
  }

  fn get_format(&self) -> Format {
    Format::Cram
  }
}

impl<S, R> CramSearch<S>
where
  S: AsyncStorage<Streamable = R> + Send + Sync + 'static,
  R: AsyncRead + AsyncSeek + Send + Sync + Unpin
{
  const FILE_DEFINITION_LENGTH: u64 = 26;
  const EOF_CONTAINER_LENGTH: u64 = 38;

  pub fn new(storage: Arc<S>) -> Self {
    Self { storage }
  }

  /// Get bytes ranges using the index.
  async fn bytes_ranges_from_index<F>(
    &self,
    key: &str,
    ref_seq: Option<&sam::header::ReferenceSequence>,
    seq_start: Option<i32>,
    seq_end: Option<i32>,
    crai_index: &[crai::Record],
    predicate: Arc<F>,
  ) -> Result<Vec<BytesRange>>
  where
    F: Fn(&Record) -> bool + Send + Sync + 'static,
  {
    // This could be improved by using some sort of index mapping.
    let mut futures = FuturesUnordered::new();
    for (record, next) in crai_index.iter().zip(crai_index.iter().skip(1)) {
      let owned_record = record.clone();
      let owned_next = next.clone();
      let ref_seq_owned = ref_seq.cloned();
      let owned_predicate = predicate.clone();
      futures.push(tokio::spawn(async move {
        if owned_predicate(&owned_record) {
          Self::bytes_ranges_for_record(
            ref_seq_owned.as_ref(),
            seq_start,
            seq_end,
            &owned_record,
            &owned_next,
          )
        } else {
          None
        }
      }));
    }

    let mut byte_ranges = Vec::new();
    loop {
      select! {
        Some(next) = futures.next() => {
          if let Some(range) = next.map_err(HtsGetError::from)? {
            byte_ranges.push(range);
          }
        },
        else => break
      }
    }

    let last = crai_index
      .last()
      .ok_or_else(|| HtsGetError::invalid_input("No entries in CRAI"))?;
    if predicate(last) {
      let file_size = self
        .storage
        .head(key)
        .await
        .map_err(|_| HtsGetError::io_error("Reading CRAM file size."))?;
      let eof_position = file_size - Self::EOF_CONTAINER_LENGTH;
      byte_ranges.push(
        BytesRange::default()
          .with_start(last.offset())
          .with_end(eof_position),
      );
    }

    Ok(BytesRange::merge_all(byte_ranges))
  }

  /// Gets bytes ranges for a specific index entry.
  pub(crate) fn bytes_ranges_for_record(
    ref_seq: Option<&sam::header::ReferenceSequence>,
    seq_start: Option<i32>,
    seq_end: Option<i32>,
    record: &Record,
    next: &Record,
  ) -> Option<BytesRange> {
    match ref_seq {
      None => Some(
        BytesRange::default()
          .with_start(record.offset())
          .with_end(next.offset()),
      ),
      Some(ref_seq) => {
        let seq_start = seq_start.unwrap_or(Self::MIN_SEQ_POSITION as i32);
        let seq_end = seq_end.unwrap_or_else(|| ref_seq.len());

        if seq_start <= record.alignment_start() + record.alignment_span()
          && seq_end >= record.alignment_start()
        {
          Some(
            BytesRange::default()
              .with_start(record.offset())
              .with_end(next.offset()),
          )
        } else {
          None
        }
      }
    }
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
      let search = CramSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878");
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![Url::new(expected_url(storage))
          .with_headers(Headers::default().with_header("Range", "bytes=6087-1627756"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_unmapped_reads() {
    with_local_storage(|storage| async move {
      let search = CramSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878").with_reference_name("*");
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![Url::new(expected_url(storage))
          .with_headers(Headers::default().with_header("Range", "bytes=1280106-1627756"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_without_seq_range() {
    with_local_storage(|storage| async move {
      let search = CramSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878").with_reference_name("20");
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![Url::new(expected_url(storage))
          .with_headers(Headers::default().with_header("Range", "bytes=604231-1280106"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_with_seq_range_no_overlap() {
    with_local_storage(|storage| async move {
      let search = CramSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878")
        .with_reference_name("11")
        .with_start(5000000)
        .with_end(5050000);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![Url::new(expected_url(storage))
          .with_headers(Headers::default().with_header("Range", "bytes=6087-465709"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_with_seq_range_overlap() {
    with_local_storage(|storage| async move {
      let search = CramSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878")
        .with_reference_name("11")
        .with_start(5000000)
        .with_end(5100000);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![Url::new(expected_url(storage))
          .with_headers(Headers::default().with_header("Range", "bytes=6087-604231"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_header() {
    with_local_storage(|storage| async move {
      let search = CramSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878").with_class(Class::Header);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![Url::new(expected_url(storage))
          .with_headers(Headers::default().with_header("Range", "bytes=26-6087"))
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
      .join("data/cram");
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
        .join("htsnexus_test_NA12878.cram")
        .to_string_lossy()
    )
  }
}
