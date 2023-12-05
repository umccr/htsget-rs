//! This module provides search capabilities for CRAM files.
//!

use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use futures_util::stream::FuturesOrdered;
use noodles::core::Position;
use noodles::cram;
use noodles::cram::crai;
use noodles::cram::crai::{Index, Record};
use noodles::sam::Header;
use tokio::io::{AsyncRead, BufReader};
use tokio::{io, select};
use tracing::{instrument, trace};

use htsget_config::types::Class::Header as HtsGetHeader;
use htsget_config::types::Interval;

use crate::htsget::search::{Search, SearchAll, SearchReads};
use crate::htsget::{ConcurrencyError, ParsedHeader};
use crate::storage::{BytesPosition, DataBlock, Storage};
use crate::Class::Body;
use crate::{Format, HtsGetError, Query, Result};

// § 9 End of file container <https://samtools.github.io/hts-specs/CRAMv3.pdf>.
static CRAM_EOF: &[u8] = &[
  0x0f, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x0f, 0xe0, 0x45, 0x4f, 0x46, 0x00, 0x00, 0x00,
  0x00, 0x01, 0x00, 0x05, 0xbd, 0xd9, 0x4f, 0x00, 0x01, 0x00, 0x06, 0x06, 0x01, 0x00, 0x01, 0x00,
  0x01, 0x00, 0xee, 0x63, 0x01, 0x4b,
];

type AsyncReader<ReaderType> = cram::AsyncReader<BufReader<ReaderType>>;

/// Allows searching through cram files.
pub struct CramSearch<S> {
  storage: Arc<S>,
}

#[async_trait]
impl<S, ReaderType>
  SearchAll<S, ReaderType, PhantomData<Self>, Index, AsyncReader<ReaderType>, Header>
  for CramSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync + 'static,
{
  #[instrument(level = "trace", skip_all, ret)]
  async fn get_byte_ranges_for_all(&self, query: &Query) -> Result<Vec<BytesPosition>> {
    Ok(vec![
      BytesPosition::default().with_end(self.position_at_eof(query).await?)
    ])
  }

  #[instrument(level = "trace", skip_all, ret)]
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

  async fn get_byte_ranges_for_header(
    &self,
    index: &Index,
    _header: &Header,
    _reader: &mut AsyncReader<ReaderType>,
    _query: &Query,
  ) -> Result<BytesPosition> {
    Ok(
      BytesPosition::default()
        .with_end(self.get_header_end_offset(index).await?)
        .with_class(HtsGetHeader),
    )
  }

  fn get_eof_marker(&self) -> &[u8] {
    CRAM_EOF
  }

  fn get_eof_data_block(&self) -> Option<DataBlock> {
    Some(DataBlock::Data(
      Vec::from(self.get_eof_marker()),
      Some(Body),
    ))
  }
}

#[async_trait]
impl<S, ReaderType>
  SearchReads<S, ReaderType, PhantomData<Self>, Index, AsyncReader<ReaderType>, Header>
  for CramSearch<S>
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync + 'static,
{
  async fn get_reference_sequence_from_name<'a>(
    &self,
    header: &'a Header,
    name: &str,
  ) -> Option<usize> {
    Some(header.reference_sequences().get_index_of(name)?)
  }

  async fn get_byte_ranges_for_unmapped_reads(
    &self,
    query: &Query,
    index: &Index,
  ) -> Result<Vec<BytesPosition>> {
    Self::bytes_ranges_from_index(
      self,
      query,
      index,
      Arc::new(|record: &Record| record.reference_sequence_id().is_none()),
    )
    .await
  }

  async fn get_byte_ranges_for_reference_sequence(
    &self,
    ref_seq_id: usize,
    query: &Query,
    index: &Index,
  ) -> Result<Vec<BytesPosition>> {
    Self::bytes_ranges_from_index(
      self,
      query,
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
  ReaderType: AsyncRead + Unpin + Send + Sync + 'static,
{
  fn init_reader(inner: ReaderType) -> AsyncReader<ReaderType> {
    AsyncReader::new(BufReader::new(inner))
  }

  async fn read_header(reader: &mut AsyncReader<ReaderType>) -> io::Result<Header> {
    reader.read_file_definition().await?;

    Ok(
      reader
        .read_file_header()
        .await?
        .parse::<ParsedHeader<Header>>()?
        .into_inner(),
    )
  }

  async fn read_index_inner<T: AsyncRead + Send + Unpin>(inner: T) -> io::Result<Index> {
    crai::AsyncReader::new(inner).read_index().await
  }

  async fn get_byte_ranges_for_reference_name(
    &self,
    reference_name: String,
    index: &Index,
    header: &Header,
    query: &Query,
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
  ReaderType: AsyncRead + Unpin + Send + Sync + 'static,
{
  /// Create the cram search.
  pub fn new(storage: Arc<S>) -> Self {
    Self { storage }
  }

  /// Get bytes ranges using the index.
  #[instrument(level = "trace", skip(self, crai_index, predicate))]
  pub async fn bytes_ranges_from_index<F>(
    &self,
    query: &Query,
    crai_index: &[Record],
    predicate: Arc<F>,
  ) -> Result<Vec<BytesPosition>>
  where
    F: Fn(&Record) -> bool + Send + Sync + 'static,
  {
    trace!("getting bytes range from index");
    // This could be improved by using some sort of index mapping.
    let mut futures = FuturesOrdered::new();
    for (record, next) in crai_index.iter().zip(crai_index.iter().skip(1)) {
      let owned_record = record.clone();
      let owned_next = next.clone();
      let owned_predicate = predicate.clone();
      let range = query.interval();
      futures.push_back(tokio::spawn(async move {
        if owned_predicate(&owned_record) {
          Self::bytes_ranges_for_record(range, &owned_record, owned_next.offset())
        } else {
          Ok(None)
        }
      }));
    }

    let mut byte_ranges = Vec::new();
    loop {
      select! {
        Some(next) = futures.next() => {
          if let Some(range) = next.map_err(ConcurrencyError::new).map_err(HtsGetError::from)?? {
            byte_ranges.push(range);
          }
        },
        else => break
      }
    }

    match crai_index.last() {
      None => {
        return Err(HtsGetError::InvalidInput(
          "No entries found in `CRAI`".to_string(),
        ));
      }
      Some(last) if predicate(last) => {
        if let Some(range) =
          Self::bytes_ranges_for_record(query.interval(), last, self.position_at_eof(query).await?)?
        {
          byte_ranges.push(range);
        }
      }
      _ => {}
    }

    Ok(byte_ranges)
  }

  /// Gets bytes ranges for a specific index entry.
  pub fn bytes_ranges_for_record(
    seq_range: Interval,
    record: &Record,
    next: u64,
  ) -> Result<Option<BytesPosition>> {
    let record_start = record.alignment_start().unwrap_or(Position::MIN);
    let record_end = record_start
      .checked_add(record.alignment_span())
      .ok_or_else(|| HtsGetError::invalid_input("adding record alignment span to `Position`"))?;

    let interval = seq_range.into_one_based()?;
    let seq_start = interval.start().unwrap_or(Position::MIN);
    let seq_end = interval.end().unwrap_or(Position::MAX);

    if seq_start <= record_end && seq_end >= record_start {
      Ok(Some(
        BytesPosition::default()
          .with_start(record.offset())
          .with_end(next)
          .with_class(Body),
      ))
    } else {
      Ok(None)
    }
  }
}

#[cfg(test)]
mod tests {
  use std::future::Future;

  use htsget_config::storage::local::LocalStorage as ConfigLocalStorage;

  #[cfg(feature = "s3-storage")]
  use crate::htsget::from_storage::tests::with_aws_storage_fn;
  use crate::htsget::from_storage::tests::with_local_storage_fn;
  use crate::storage::local::LocalStorage;
  use crate::{Class::Header, Headers, HtsGetError::NotFound, Response, Url};

  use super::*;

  const DATA_LOCATION: &str = "data/cram";
  const INDEX_FILE_LOCATION: &str = "htsnexus_test_NA12878.cram.crai";

  #[tokio::test]
  async fn search_all_reads() {
    with_local_storage(|storage| async move {
      let search = CramSearch::new(storage.clone());
      let query = Query::new_with_defaults("htsnexus_test_NA12878", Format::Cram);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![Url::new(expected_url())
          .with_headers(Headers::default().with_header("Range", "bytes=0-1627793"))],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_unmapped_reads() {
    with_local_storage(|storage| async move {
      let search = CramSearch::new(storage.clone());
      let query =
        Query::new_with_defaults("htsnexus_test_NA12878", Format::Cram).with_reference_name("*");
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-6086"))
            .with_class(Header),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=1280106-1627793"))
            .with_class(Body),
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
      let query =
        Query::new_with_defaults("htsnexus_test_NA12878", Format::Cram).with_reference_name("20");
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-6086"))
            .with_class(Header),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=604231-1280105"))
            .with_class(Body),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=1627756-1627793"))
            .with_class(Body),
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
      let query = Query::new_with_defaults("htsnexus_test_NA12878", Format::Cram)
        .with_reference_name("11")
        .with_start(5000000)
        .with_end(5050000);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=0-465708")),
          Url::new(expected_url())
            .with_headers(Headers::default().with_header("Range", "bytes=1627756-1627793")),
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
      let query = Query::new_with_defaults("htsnexus_test_NA12878", Format::Cram)
        .with_reference_name("11")
        .with_start(5000000)
        .with_end(5100000);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(expected_response_with_start());
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_reference_name_with_no_end_position() {
    with_local_storage(|storage| async move {
      let search = CramSearch::new(storage.clone());
      let query = Query::new_with_defaults("htsnexus_test_NA12878", Format::Cram)
        .with_reference_name("11")
        .with_start(5000000);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(expected_response_with_start());
      assert_eq!(response, expected_response)
    })
    .await;
  }

  fn expected_response_with_start() -> Response {
    Response::new(
      Format::Cram,
      vec![
        Url::new(expected_url())
          .with_headers(Headers::default().with_header("Range", "bytes=0-604230")),
        Url::new(expected_url())
          .with_headers(Headers::default().with_header("Range", "bytes=1627756-1627793")),
      ],
    )
  }

  #[tokio::test]
  async fn search_header() {
    with_local_storage(|storage| async move {
      let search = CramSearch::new(storage.clone());
      let query =
        Query::new_with_defaults("htsnexus_test_NA12878", Format::Cram).with_class(Header);
      let response = search.search(query).await;
      println!("{response:#?}");

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![Url::new(expected_url())
          .with_headers(Headers::default().with_header("Range", "bytes=0-6086"))
          .with_class(Header)],
      ));
      assert_eq!(response, expected_response)
    })
    .await;
  }

  #[tokio::test]
  async fn search_non_existent_id_reference_name() {
    with_local_storage_fn(
      |storage| async move {
        let search = CramSearch::new(storage.clone());
        let query = Query::new_with_defaults("htsnexus_test_NA12878", Format::Cram);
        let response = search.search(query).await;
        assert!(matches!(response, Err(NotFound(_))));
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
        let search = CramSearch::new(storage.clone());
        let query =
          Query::new_with_defaults("htsnexus_test_NA12878", Format::Cram).with_reference_name("20");
        let response = search.search(query).await;
        assert!(matches!(response, Err(NotFound(_))));
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
        let search = CramSearch::new(storage.clone());
        let query =
          Query::new_with_defaults("htsnexus_test_NA12878", Format::Cram).with_class(Header);
        let response = search.search(query).await;
        assert!(matches!(response, Err(NotFound(_))));
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
        let search = CramSearch::new(storage);
        let query = Query::new_with_defaults("htsnexus_test_NA12878", Format::Cram);
        let response = search.search(query).await;
        assert!(response.is_err());
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
        let search = CramSearch::new(storage);
        let query =
          Query::new_with_defaults("htsnexus_test_NA12878", Format::Cram).with_reference_name("20");
        let response = search.search(query).await;
        assert!(response.is_err());
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
        let search = CramSearch::new(storage);
        let query =
          Query::new_with_defaults("htsnexus_test_NA12878", Format::Cram).with_class(Header);
        let response = search.search(query).await;
        assert!(response.is_err());
      },
      DATA_LOCATION,
      &[INDEX_FILE_LOCATION],
    )
    .await
  }

  async fn with_local_storage<F, Fut>(test: F)
  where
    F: FnOnce(Arc<LocalStorage<ConfigLocalStorage>>) -> Fut,
    Fut: Future<Output = ()>,
  {
    with_local_storage_fn(test, "data/cram", &[]).await
  }

  fn expected_url() -> String {
    "http://127.0.0.1:8081/data/htsnexus_test_NA12878.cram".to_string()
  }
}
