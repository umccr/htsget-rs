//! This module provides search capabilities for CRAM files.
//!

use std::marker::PhantomData;
use std::ops::Range;
use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use futures_util::stream::FuturesOrdered;
use noodles::cram::crai;
use noodles::cram::crai::{Index, Record};
use noodles::sam;
use noodles::sam::Header;
use noodles_cram::AsyncReader;
use tokio::io::AsyncRead;
use tokio::{io, select};

use crate::htsget::search::{into_one_based_position, Search, SearchAll, SearchReads};
use crate::htsget::{Format, HtsGetError, Query, Result};
use crate::storage::{BytesPosition, DataBlock, Storage};

// § 9 End of file container <https://samtools.github.io/hts-specs/CRAMv3.pdf>.
static CRAM_EOF: &[u8] = &[
  0x0f, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x0f, 0xe0, 0x45, 0x4f, 0x46, 0x00, 0x00, 0x00,
  0x00, 0x01, 0x00, 0x05, 0xbd, 0xd9, 0x4f, 0x00, 0x01, 0x00, 0x06, 0x06, 0x01, 0x00, 0x01, 0x00,
  0x01, 0x00, 0xee, 0x63, 0x01, 0x4b,
];

pub(crate) struct CramSearch<S> {
  storage: Arc<S>,
}

#[async_trait]
impl<S, ReaderType>
  SearchAll<S, ReaderType, PhantomData<Self>, Index, AsyncReader<ReaderType>, Header>
  for CramSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
{
  async fn get_byte_ranges_for_all(
    &self,
    id: String,
    format: Format,
    index: &Index,
  ) -> Result<Vec<BytesPosition>> {
    Self::bytes_ranges_from_index(
      self,
      &id,
      &format,
      None,
      Range::default(),
      index,
      Arc::new(|_: &Record| true),
    )
    .await
  }

  async fn get_header_end_offset(&self, index: &Index) -> Result<u64> {
    // Does the first index entry always contain the first data container?
    index
      .iter()
      .min_by(|x, y| x.offset().cmp(&y.offset()))
      .map(|min_record| min_record.offset())
      .ok_or_else(|| {
        HtsGetError::io_error(format!(
          "Failed to find entry in {} index",
          self.get_format()
        ))
      })
  }

  fn get_eof_marker(&self) -> &[u8] {
    CRAM_EOF
  }

  fn get_eof_data_block(&self) -> Option<DataBlock> {
    Some(DataBlock::Data(Vec::from(self.get_eof_marker())))
  }
}

#[async_trait]
impl<S, ReaderType>
  SearchReads<S, ReaderType, PhantomData<Self>, Index, AsyncReader<ReaderType>, Header>
  for CramSearch<S>
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
    index: &Index,
  ) -> Result<Vec<BytesPosition>> {
    Self::bytes_ranges_from_index(
      self,
      &query.id,
      &self.get_format(),
      None,
      Range::default(),
      index,
      Arc::new(|record: &Record| record.reference_sequence_id().is_none()),
    )
    .await
  }

  async fn get_byte_ranges_for_reference_sequence(
    &self,
    ref_seq: &sam::header::ReferenceSequence,
    ref_seq_id: usize,
    query: Query,
    index: &Index,
  ) -> Result<Vec<BytesPosition>> {
    Self::bytes_ranges_from_index(
      self,
      &query.id,
      &self.get_format(),
      Some(ref_seq),
      query
        .start
        .map(|start| start as i32)
        .map(into_one_based_position)
        .transpose()?
        .unwrap_or(Self::MIN_SEQ_POSITION as i32)
        ..query
          .end
          .map(|end| end as i32)
          .map(into_one_based_position)
          .transpose()?
          .unwrap_or(ref_seq.len().get() as i32),
      index,
      Arc::new(move |record: &Record| record.reference_sequence_id() == Some(ref_seq_id)),
    )
    .await
  }
}

/// PhantomData is used because of a lack of reference sequence data for CRAM.
#[async_trait]
impl<S, ReaderType> Search<S, ReaderType, PhantomData<Self>, Index, AsyncReader<ReaderType>, Header>
  for CramSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
{
  fn init_reader(inner: ReaderType) -> AsyncReader<ReaderType> {
    AsyncReader::new(inner)
  }

  async fn read_raw_header(reader: &mut AsyncReader<ReaderType>) -> io::Result<String> {
    reader.read_file_definition().await?;
    reader.read_file_header().await
  }

  async fn read_index_inner<T: AsyncRead + Send + Unpin>(inner: T) -> io::Result<Index> {
    crai::AsyncReader::new(inner).read_index().await
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
    self.storage.clone()
  }

  fn get_format(&self) -> Format {
    Format::Cram
  }
}

impl<S, ReaderType> CramSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
{
  const EOF_CONTAINER_LENGTH: u64 = 38;

  pub fn new(storage: Arc<S>) -> Self {
    Self { storage }
  }

  /// Get bytes ranges using the index.
  async fn bytes_ranges_from_index<F>(
    &self,
    id: &str,
    format: &Format,
    ref_seq: Option<&sam::header::ReferenceSequence>,
    seq_range: Range<i32>,
    crai_index: &[Record],
    predicate: Arc<F>,
  ) -> Result<Vec<BytesPosition>>
  where
    F: Fn(&Record) -> bool + Send + Sync + 'static,
  {
    // This could be improved by using some sort of index mapping.
    let mut futures = FuturesOrdered::new();
    for (record, next) in crai_index.iter().zip(crai_index.iter().skip(1)) {
      let owned_record = record.clone();
      let owned_next = next.clone();
      let ref_seq_owned = ref_seq.cloned();
      let owned_predicate = predicate.clone();
      let range = seq_range.clone();
      futures.push(tokio::spawn(async move {
        if owned_predicate(&owned_record) {
          Self::bytes_ranges_for_record(ref_seq_owned.as_ref(), range, &owned_record, &owned_next)
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
        .head(format.fmt_file(id))
        .await
        .map_err(|_| HtsGetError::io_error("Reading CRAM file size."))?;
      let eof_position = file_size - Self::EOF_CONTAINER_LENGTH;
      byte_ranges.push(
        BytesPosition::default()
          .with_start(last.offset())
          .with_end(eof_position),
      );
    }

    Ok(BytesPosition::merge_all(byte_ranges))
  }

  /// Gets bytes ranges for a specific index entry.
  pub(crate) fn bytes_ranges_for_record(
    ref_seq: Option<&sam::header::ReferenceSequence>,
    seq_range: Range<i32>,
    record: &Record,
    next: &Record,
  ) -> Option<BytesPosition> {
    match ref_seq {
      None => Some(
        BytesPosition::default()
          .with_start(record.offset())
          .with_end(next.offset()),
      ),
      Some(_) => {
        let start = record
          .alignment_start()
          .map(usize::from)
          .unwrap_or_default() as i32;
        if seq_range.start <= start + record.alignment_span() as i32 && seq_range.end >= start {
          Some(
            BytesPosition::default()
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
mod tests {
  use std::future::Future;

  use htsget_test_utils::util::expected_cram_eof_data_url;

  use crate::htsget::from_storage::tests::with_local_storage as with_local_storage_path;
  use crate::htsget::{Class, Class::Body, Headers, Response, Url};
  use crate::storage::local::LocalStorage;
  use crate::storage::ticket_server::HttpTicketFormatter;

  use super::*;

  #[tokio::test]
  async fn search_all_reads() {
    with_local_storage(|storage| async move {
      let search = CramSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878", Format::Cram);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-1627755")),
          Url::new(expected_cram_eof_data_url()).with_class(Body),
        ],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_unmapped_reads() {
    with_local_storage(|storage| async move {
      let search = CramSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878", Format::Cram).with_reference_name("*");
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-6086")),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=1280106-1627755")),
          Url::new(expected_cram_eof_data_url()).with_class(Body),
        ],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_without_seq_range() {
    with_local_storage(|storage| async move {
      let search = CramSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878", Format::Cram).with_reference_name("20");
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-6086")),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=604231-1280105")),
          Url::new(expected_cram_eof_data_url()).with_class(Body),
        ],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_with_seq_range_no_overlap() {
    with_local_storage(|storage| async move {
      let search = CramSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878", Format::Cram)
        .with_reference_name("11")
        .with_start(5000000)
        .with_end(5050000);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-465708")),
          Url::new(expected_cram_eof_data_url()).with_class(Body),
        ],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_with_seq_range_overlap() {
    with_local_storage(|storage| async move {
      let search = CramSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878", Format::Cram)
        .with_reference_name("11")
        .with_start(5000000)
        .with_end(5100000);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-604230")),
          Url::new(expected_cram_eof_data_url()).with_class(Body),
        ],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_header() {
    with_local_storage(|storage| async move {
      let search = CramSearch::new(storage.clone());
      let query = Query::new("htsnexus_test_NA12878", Format::Cram).with_class(Class::Header);
      let response = search.search(query).await;
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![Url::new(expected_url())
          .with_headers(Headers::default().with_header("Range", "bytes=0-6086"))
          .with_class(Class::Header)],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  async fn with_local_storage<F, Fut>(test: F)
  where
    F: FnOnce(Arc<LocalStorage<HttpTicketFormatter>>) -> Fut,
    Fut: Future<Output = ()>,
  {
    with_local_storage_path(test, "data/cram").await
  }

  fn expected_url() -> String {
    "http://127.0.0.1:8081/data/htsnexus_test_NA12878.cram".to_string()
  }
}
