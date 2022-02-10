//! The following file defines commonalities between all the file formats. While each format has
//! its own particularities, there are many shared components that can be abstracted.
//!
//! The generic types represent the specifics of the formats, and allow the abstractions to be made,
//! where the names of the types indicate their purpose.
//!

use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use noodles::bgzf::VirtualPosition;
use noodles::csi::binning_index::merge_chunks;
use noodles::csi::{BinningIndex, BinningIndexReferenceSequence};
use noodles::sam;
use noodles_bam::AsyncReader;
use tokio::fs::File;
use tokio::io;
use tokio::io::AsyncRead;
use tokio::select;
use tokio::task::JoinHandle;

use crate::storage::GetOptions;
use crate::{
  htsget::{Class, Format, HtsGetError, Query, Response, Result},
  storage::{AsyncStorage, BytesRange, UrlOptions},
};

// pub(crate) type AsyncHeaderResult = io::Result<String>;
// pub(crate) type AsyncIndexResult<'a, Index> = io::Result<Index>;

/// Helper function to find the first non-none value from a set of futures.
pub(crate) async fn find_first<T>(
  msg: &str,
  mut futures: FuturesUnordered<JoinHandle<Option<T>>>,
) -> Result<T> {
  let mut result = None;
  loop {
    select! {
      Some(next) = futures.next() => {
        if let Some(next) = next.map_err(HtsGetError::from)? {
          result = Some(next);
          break;
        }
      },
      else => break
    };
  }
  result.ok_or_else(|| HtsGetError::not_found(msg))
}

/// [SearchAll] represents searching bytes ranges that are applicable to all formats. Specifically,
/// range for the whole file, and the header.
///
/// [ReferenceSequence] is the reference sequence type of the format's index.
/// [Index] is the format's index type.
/// [Reader] is the format's reader type.
/// [Header] is the format's header type.
#[async_trait]
pub(crate) trait SearchAll<S, ReferenceSequence, Index, Reader, Header>
where
  Index: Send + Sync,
{
  /// This returns mapped and placed unmapped ranges.
  async fn get_byte_ranges_for_all(&self, key: String, index: &Index) -> Result<Vec<BytesRange>>;

  /// Returns the header bytes range.
  async fn get_byte_ranges_for_header(&self, key: &str) -> Result<Vec<BytesRange>>;
}

/// [SearchReads] represents searching bytes ranges for the reads endpoint.
///
/// [ReferenceSequence] is the reference sequence type of the format's index.
/// [Index] is the format's index type.
/// [Reader] is the format's reader type.
/// [Header] is the format's header type.
#[async_trait]
pub(crate) trait SearchReads<S, ReferenceSequence, Index, AsyncReader, Header>:
  Search<S, ReferenceSequence, Index, AsyncReader, Header>
where
  S: AsyncStorage + Send + Sync + 'static,
  AsyncReader: Send,
  Header: FromStr + Send + Sync,
  Index: Send + Sync,
{
  /// Get reference sequence from name.
  async fn get_reference_sequence_from_name<'b>(
    &self,
    header: &'b Header,
    name: &str,
  ) -> Option<(usize, &'b String, &'b sam::header::ReferenceSequence)>;

  /// Get unplaced unmapped ranges.
  async fn get_byte_ranges_for_unmapped_reads(
    &self,
    key: &str,
    index: &Index,
  ) -> Result<Vec<BytesRange>>;

  /// Get reads ranges for a reference sequence implementation.
  async fn get_byte_ranges_for_reference_sequence(
    &self,
    key: String,
    reference_sequence: &sam::header::ReferenceSequence,
    ref_seq_id: usize,
    query: &Query,
    index: &Index,
  ) -> Result<Vec<BytesRange>>;

  ///Get reads for a given reference name and an optional sequence range.
  async fn get_byte_ranges_for_reference_name_reads(
    &self,
    key: String,
    reference_name: &str,
    index: &Index,
    query: &Query,
  ) -> Result<Vec<BytesRange>> {
    if reference_name == "*" {
      return self.get_byte_ranges_for_unmapped_reads(&key, index).await;
    }

    let (_, header) = self.create_reader(&key).await?;
    let maybe_ref_seq = self
      .get_reference_sequence_from_name(&header, reference_name)
      .await;

    let byte_ranges = match maybe_ref_seq {
      None => Err(HtsGetError::not_found(format!(
        "Reference name not found: {}",
        reference_name
      ))),
      Some((bam_ref_seq_idx, _, bam_ref_seq)) => {
        Self::get_byte_ranges_for_reference_sequence(
          self,
          key,
          bam_ref_seq,
          bam_ref_seq_idx,
          query,
          index,
        )
        .await
      }
    }?;
    Ok(byte_ranges)
  }
}

/// [Search] is the general trait that all formats implement, including functions from [SearchAll].
///
/// [ReferenceSequence] is the reference sequence type of the format's index.
/// [Index] is the format's index type.
/// [Reader] is the format's reader type.
/// [Header] is the format's header type.
#[async_trait]
pub(crate) trait Search<S, ReferenceSequence, Index, Reader, Header>:
  SearchAll<S, ReferenceSequence, Index, Reader, Header>
where
  S: AsyncStorage + Send + Sync,
  Header: FromStr + Send,
  Reader: Send,
  Index: Send + Sync,
  Self: Sync + Send,
{
  const MIN_SEQ_POSITION: u32 = 1; // 1-based

  // const READER_FN: async { fn(AsyncReader<AsyncRead>) -> io::AsyncRead };
  // const HEADER_FN: async fn(&'_ mut io::AsyncRead) -> AsyncHeaderResult;
  // const INDEX_FN: async fn(AsyncRead) -> AsyncIndexResult<'static, Index>;

  async fn read(reader: AsyncReader::<dyn AsyncRead>) -> dyn io::AsyncRead;
  async fn header(header: AsyncReader::<dyn AsyncRead>) -> dyn io::AsyncRead;
  async fn index(index: AsyncReader::<dyn AsyncRead>) -> dyn io::AsyncRead;

  /// Get ranges for a given reference name and an optional sequence range.
  async fn get_byte_ranges_for_reference_name(
    &self,
    key: String,
    reference_name: String,
    index: &Index,
    query: &Query,
  ) -> Result<Vec<BytesRange>>;

  /// Generate a key for the storage object from an ID
  /// This may involve a more complex transformation in the future,
  /// or even require custom implementations depending on the organizational structure
  /// For now there is a 1:1 mapping to the underlying files
  fn get_keys_from_id(&self, id: &str) -> (String, String);

  /// Get the storage of this trait.
  fn get_storage(&self) -> Arc<S>;

  /// Get the format of this trait.
  fn get_format(&self) -> Format;

  /// Read the index from the key.
  async fn read_index(&self, key: &str) -> Result<Index> {
    let path = self.get_storage().get(&key, GetOptions::default()).await?;
    Self::INDEX_FN(path)
      .await
      .map_err(|_| HtsGetError::io_error(format!("Reading {} index file", self.get_format())))
  }

  /// Search based on the query.
  async fn search(&self, query: Query) -> Result<Response> {
    let (file_key, index_key) = self.get_keys_from_id(query.id.as_str());

    match query.class {
      Class::Body => {
        let index = self.read_index(&index_key).await?;

        let byte_ranges = match query.reference_name.as_ref() {
          None => {
            self
              .get_byte_ranges_for_all(file_key.clone(), &index)
              .await?
          }
          Some(reference_name) => {
            self
              .get_byte_ranges_for_reference_name(
                file_key.clone(),
                reference_name.clone(),
                &index,
                &query,
              )
              .await?
          }
        };
        self.build_response(query, file_key, byte_ranges).await
      }
      Class::Header => {
        let byte_ranges = self.get_byte_ranges_for_header(&file_key).await?;
        self.build_response(query, file_key, byte_ranges).await
      }
    }
  }

  /// Build the response from the query using urls.
  async fn build_response(
    &self,
    query: Query,
    key: String,
    byte_ranges: Vec<BytesRange>,
  ) -> Result<Response> {
    let mut storage_futures = FuturesUnordered::new();
    for range in byte_ranges {
      let options = UrlOptions::default()
        .with_range(range)
        .with_class(query.class.clone());
      let storage = self.get_storage();
      let storage_key = key.clone();
      storage_futures.push(tokio::spawn(async move {
        storage.url(storage_key, options).await
      }));
    }
    let mut urls = Vec::new();
    loop {
      select! {
        Some(next) = storage_futures.next() => urls.push(next.map_err(HtsGetError::from)?.map_err(HtsGetError::from)?),
        else => break
      }
    }
    let format = query.format.unwrap_or_else(|| self.get_format());
    return Ok(Response::new(format, urls));
  }

  /// Get the reader from the key.
  async fn reader<U>(key: &str, msg: U, storage: Arc<S>) -> Result<Reader>
  where
    U: Into<String> + Send,
  {
    let get_options = GetOptions::default();
    let path = storage.get(key, get_options).await?;

    File::open(path)
      .await
      .map(Self::READER_FN)
      .map_err(|_| HtsGetError::io_error(msg))
  }

  /// Get the reader and header using the key.
  async fn create_reader(&self, key: &str) -> Result<(Reader, Header)> {
    let mut reader = Self::reader(
      key,
      format!("Reading {}", self.get_format()),
      self.get_storage(),
    )
    .await?;

    let header = Self::HEADER_FN(&mut reader)
      .map_err(|_| HtsGetError::io_error(format!("Reading {} header", self.get_format())))?
      .parse::<Header>()
      .map_err(|_| HtsGetError::io_error(format!("Parsing {} header", self.get_format())))?;

    Ok((reader, header))
  }
}

/// The [BgzfSearch] trait defines commonalities for the formats that use a binning index, specifically
/// BAM, BCF, and VCF.
///
/// [ReferenceSequence] is the reference sequence type of the format's index.
/// [Index] is the format's index type.
/// [Reader] is the format's reader type.
/// [Header] is the format's header type.
#[async_trait]
pub(crate) trait BgzfSearch<S, ReferenceSequence, Index, Reader, Header>:
  Search<S, ReferenceSequence, Index, Reader, Header>
where
  S: AsyncStorage + Send + Sync + 'static,
  Reader: BlockPosition + Send,
  ReferenceSequence: BinningIndexReferenceSequence,
  Index: BinningIndex<ReferenceSequence> + Send + Sync,
  Header: FromStr + Send,
{
  type ReferenceSequenceHeader: Sync;

  /// Get the max sequence position.
  fn max_seq_position(ref_seq: &Self::ReferenceSequenceHeader) -> i32;

  /// Get ranges for a reference sequence for the bgzf format.
  async fn get_byte_ranges_for_reference_sequence_bgzf(
    &self,
    key: String,
    reference_sequence: &Self::ReferenceSequenceHeader,
    ref_seq_id: usize,
    index: &Index,
    seq_start: Option<i32>,
    seq_end: Option<i32>,
  ) -> Result<Vec<BytesRange>> {
    let seq_start = seq_start.unwrap_or(Self::MIN_SEQ_POSITION as i32);
    let seq_end = seq_end.unwrap_or_else(|| Self::max_seq_position(reference_sequence));

    // TODO convert to async if supported later.
    let chunks = index
      .query(ref_seq_id, seq_start..=seq_end)
      .map_err(|_| HtsGetError::InvalidRange(format!("{}-{}", seq_start, seq_end)))?;

    let mut futures: FuturesUnordered<JoinHandle<Result<BytesRange>>> = FuturesUnordered::new();
    for chunk in merge_chunks(&chunks) {
      let storage = self.get_storage();
      let storage_key = key.clone();
      futures.push(tokio::spawn(async move {
        let mut reader = Self::reader(&storage_key, "Reading BGZF", storage).await?;
        Ok(
          BytesRange::default()
            .with_start(chunk.start().bytes_range_start())
            .with_end(chunk.end().bytes_range_end(&mut reader).await),
        )
      }));
    }

    let mut byte_ranges = Vec::new();
    loop {
      select! {
        Some(next) = futures.next() => byte_ranges.push(next.map_err(HtsGetError::from)?.map_err(HtsGetError::from)?),
        else => break
      }
    }

    Ok(BytesRange::merge_all(byte_ranges))
  }

  /// Get unmapped bytes ranges.
  async fn get_byte_ranges_for_unmapped(
    &self,
    _key: &str,
    _index: &Index,
  ) -> Result<Vec<BytesRange>> {
    Ok(Vec::new())
  }
}

#[async_trait]
impl<S, ReferenceSequence, Index, Reader, Header, T>
  SearchAll<S, ReferenceSequence, Index, Reader, Header> for T
where
  S: AsyncStorage + Send + Sync + 'static,
  Reader: BlockPosition + Send,
  Header: FromStr + Send,
  ReferenceSequence: BinningIndexReferenceSequence + Sync,
  Index: BinningIndex<ReferenceSequence> + Send + Sync,
  T: BgzfSearch<S, ReferenceSequence, Index, Reader, Header> + Send + Sync,
{
  async fn get_byte_ranges_for_all(&self, key: String, index: &Index) -> Result<Vec<BytesRange>> {
    let mut futures: FuturesUnordered<JoinHandle<Result<BytesRange>>> = FuturesUnordered::new();
    for ref_sequences in index.reference_sequences() {
      if let Some(metadata) = ref_sequences.metadata() {
        let storage = self.get_storage();
        let storage_key = key.clone();
        let start_vpos = metadata.start_position();
        let end_vpos = metadata.end_position();
        futures.push(tokio::spawn(async move {
          let mut reader = Self::reader(&storage_key, "Reading BGZF", storage).await?;
          let start_vpos = start_vpos.bytes_range_start();
          let end_vpos = end_vpos.bytes_range_end(&mut reader).await;

          Ok(
            BytesRange::default()
              .with_start(start_vpos)
              .with_end(end_vpos),
          )
        }));
      }
    }

    let mut byte_ranges = Vec::new();
    loop {
      select! {
        Some(next) = futures.next() => byte_ranges.push(next.map_err(HtsGetError::from)?.map_err(HtsGetError::from)?),
        else => break
      }
    }

    let unmapped_byte_ranges = self.get_byte_ranges_for_unmapped(&key, index).await?;
    byte_ranges.extend(unmapped_byte_ranges.into_iter());
    Ok(BytesRange::merge_all(byte_ranges))
  }

  async fn get_byte_ranges_for_header(&self, key: &str) -> Result<Vec<BytesRange>> {
    let (mut reader, _) = self.create_reader(key).await?;
    let virtual_position = reader.virtual_position();
    Ok(vec![BytesRange::default().with_start(0).with_end(
      virtual_position.bytes_range_end(&mut reader).await,
    )])
  }
}

/// A block position extends the concept of a virtual position for readers.
#[async_trait]
pub(crate) trait BlockPosition {
  /// Read bytes of record.
  async fn read_bytes(&mut self) -> Option<usize>;
  /// Seek using VirtualPosition.
  async fn seek(&mut self, pos: VirtualPosition) -> io::Result<VirtualPosition>;
  /// Read the virtual position.
  fn virtual_position(&self) -> VirtualPosition;
}

/// An extension trait for VirtualPosition, which defines some common functions for the Bgzf formats.
#[async_trait]
pub(crate) trait VirtualPositionExt {
  const MAX_BLOCK_SIZE: u64 = 65536;

  /// Get the starting bytes for a compressed BGZF block.
  fn bytes_range_start(&self) -> u64;
  /// Get the ending bytes for a compressed BGZF block.
  async fn bytes_range_end<P>(&self, reader: &mut P) -> u64
  where
    P: BlockPosition + Send;
  /// Get the next block position
  async fn get_next_block_position<P>(&self, reader: &mut P) -> Option<u64>
  where
    P: BlockPosition + Send;
  fn to_string(&self) -> String;
}

#[async_trait]
impl VirtualPositionExt for VirtualPosition {
  /// This is just an alias to compressed. Kept for consistency.
  fn bytes_range_start(&self) -> u64 {
    self.compressed()
  }
  /// The compressed part refers always to the beginning of a BGZF block.
  /// But when we need to translate it into a byte range, we need to make sure
  /// the reads falling inside that block are also included, which requires to know
  /// where that block ends, which is not trivial nor possible for the last block.
  /// The solution used here goes through reading the records starting at the compressed
  /// virtual offset (coffset) of the end position (remember this will always be the
  /// start of a BGZF block). If we read the records pointed by that coffset until we
  /// reach a different coffset, we can find out where the current block ends.
  /// Therefore this can be used to only add the required bytes in the query results.
  /// If for some reason we can't read correctly the records we fall back
  /// to adding the maximum BGZF block size.
  async fn bytes_range_end<P>(&self, reader: &mut P) -> u64
  where
    P: BlockPosition + Send,
  {
    if self.uncompressed() == 0 {
      // If the uncompressed part is exactly zero, we don't need the next block
      return self.compressed();
    }
    self
      .get_next_block_position(reader)
      .await
      .unwrap_or(self.compressed() + Self::MAX_BLOCK_SIZE)
  }

  /// Get the next block position from the reader.
  async fn get_next_block_position<P>(&self, reader: &mut P) -> Option<u64>
  where
    P: BlockPosition + Send,
  {
    reader.seek(*self).await.ok()?;
    let next_block_index = loop {
      let bytes_read = reader.read_bytes().await?;
      let actual_block_index = reader.virtual_position().compressed();
      if bytes_read == 0 || actual_block_index > self.compressed() {
        break actual_block_index;
      }
    };
    Some(next_block_index)
  }

  /// Convert to string.
  fn to_string(&self) -> String {
    format!("{}/{}", self.compressed(), self.uncompressed())
  }
}
