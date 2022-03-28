//! The following file defines commonalities between all the file formats. While each format has
//! its own particularitieS, there are many shared components that can be abstracted.
//!
//! The generic types represent the specifics of the formatS, and allow the abstractions to be made,
//! where the names of the types indicate their purpose.
//!

use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use noodles::bgzf::VirtualPosition;
use noodles::core::Position;
use noodles::csi::binning_index::merge_chunks;
use noodles::csi::{BinningIndex, BinningIndexReferenceSequence};
use noodles::sam;
use tokio::io;
use tokio::io::AsyncRead;
use tokio::select;
use tokio::task::JoinHandle;

use crate::storage::GetOptions;
use crate::{
  htsget::{Class, Format, HtsGetError, Query, Response, Result},
  storage::{AsyncStorage, BytesRange, UrlOptions},
};

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
/// [S] is the storage type.
/// [ReaderType] is the inner type used for [Reader].
/// [ReferenceSequence] is the reference sequence type of the format's index.
/// [Index] is the format's index type.
/// [Reader] is the format's reader type.
/// [Header] is the format's header type.
#[async_trait]
pub(crate) trait SearchAll<S, ReaderType, ReferenceSequence, Index, Reader, Header>
where
  Index: Send + Sync,
{
  /// This returns mapped and placed unmapped ranges.
  async fn get_byte_ranges_for_all(
    &self,
    id: String,
    format: Format,
    index: &Index,
  ) -> Result<Vec<BytesRange>>;

  /// Returns the header bytes range.
  async fn get_byte_ranges_for_header(&self, query: &Query) -> Result<Vec<BytesRange>>;
}

/// [SearchReads] represents searching bytes ranges for the reads endpoint.
///
/// [S] is the storage type.
/// [ReaderType] is the inner type used for [Reader].
/// [ReferenceSequence] is the reference sequence type of the format's index.
/// [Index] is the format's index type.
/// [Reader] is the format's reader type.
/// [Header] is the format's header type.
#[async_trait]
pub(crate) trait SearchReads<S, ReaderType, ReferenceSequence, Index, Reader, Header>:
  Search<S, ReaderType, ReferenceSequence, Index, Reader, Header>
where
  S: AsyncStorage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
  Reader: Send,
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
    query: &Query,
    index: &Index,
  ) -> Result<Vec<BytesRange>>;

  /// Get reads ranges for a reference sequence implementation.
  async fn get_byte_ranges_for_reference_sequence(
    &self,
    reference_sequence: &sam::header::ReferenceSequence,
    ref_seq_id: usize,
    query: Query,
    index: &Index,
  ) -> Result<Vec<BytesRange>>;

  ///Get reads for a given reference name and an optional sequence range.
  async fn get_byte_ranges_for_reference_name_reads(
    &self,
    reference_name: &str,
    index: &Index,
    query: Query,
  ) -> Result<Vec<BytesRange>> {
    if reference_name == "*" {
      return self.get_byte_ranges_for_unmapped_reads(&query, index).await;
    }

    let (_, header) = self.create_reader(&query.id, &self.get_format()).await?;
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
/// [S] is the storage type.
/// [ReaderType] is the inner type used for [Reader].
/// [ReferenceSequence] is the reference sequence type of the format's index.
/// [Index] is the format's index type.
/// [Reader] is the format's reader type.
/// [Header] is the format's header type.
#[async_trait]
pub(crate) trait Search<S, ReaderType, ReferenceSequence, Index, Reader, Header>:
  SearchAll<S, ReaderType, ReferenceSequence, Index, Reader, Header>
where
  S: AsyncStorage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
  Index: Send + Sync,
  Header: FromStr + Send,
  Reader: Send,
  Self: Sync + Send,
{
  const MIN_SEQ_POSITION: u32 = 1; // 1-based

  fn init_reader(inner: ReaderType) -> Reader;
  async fn read_raw_header(reader: &mut Reader) -> io::Result<String>;
  async fn read_index_inner<T: AsyncRead + Unpin + Send>(inner: T) -> io::Result<Index>;

  /// Get ranges for a given reference name and an optional sequence range.
  async fn get_byte_ranges_for_reference_name(
    &self,
    reference_name: String,
    index: &Index,
    query: Query,
  ) -> Result<Vec<BytesRange>>;

  /// Get the storage of this trait.
  fn get_storage(&self) -> Arc<S>;

  /// Get the format of this trait.
  fn get_format(&self) -> Format;

  /// Read the index from the key.
  async fn read_index(&self, id: &str) -> Result<Index> {
    let storage = self
      .get_storage()
      .get(self.get_format().fmt_index(id), GetOptions::default())
      .await?;
    Self::read_index_inner(storage)
      .await
      .map_err(|err| HtsGetError::io_error(format!("Reading {} index: {}", self.get_format(), err)))
  }

  /// Search based on the query.
  async fn search(&self, query: Query) -> Result<Response> {
    match query.class {
      Class::Body => {
        let index = self.read_index(&query.id).await?;

        let format = self.get_format();
        let id = query.id.clone();
        let class = query.class.clone();
        let byte_ranges = match query.reference_name.as_ref() {
          None => {
            self
              .get_byte_ranges_for_all(query.id.clone(), format, &index)
              .await?
          }
          Some(reference_name) => {
            self
              .get_byte_ranges_for_reference_name(reference_name.clone(), &index, query)
              .await?
          }
        };
        self.build_response(class, id, format, byte_ranges).await
      }
      Class::Header => {
        let byte_ranges = self.get_byte_ranges_for_header(&query).await?;
        self
          .build_response(query.class, query.id, self.get_format(), byte_ranges)
          .await
      }
    }
  }

  /// Build the response from the query using urls.
  async fn build_response(
    &self,
    class: Class,
    id: String,
    format: Format,
    byte_ranges: Vec<BytesRange>,
  ) -> Result<Response> {
    let mut storage_futures = FuturesUnordered::new();
    for range in byte_ranges {
      let options = UrlOptions::default()
        .with_range(range)
        .with_class(class.clone());
      let storage = self.get_storage();
      let id = id.clone();
      storage_futures.push(tokio::spawn(async move {
        storage.url(format.fmt_file(&id), options).await
      }));
    }
    let mut urls = Vec::new();
    loop {
      select! {
        Some(next) = storage_futures.next() => urls.push(next.map_err(HtsGetError::from)?.map_err(HtsGetError::from)?),
        else => break
      }
    }
    return Ok(Response::new(format, urls));
  }

  /// Get the reader from the key.
  async fn reader(id: &str, format: &Format, storage: Arc<S>) -> Result<Reader> {
    let get_options = GetOptions::default();
    let storage = storage.get(format.fmt_file(id), get_options).await?;
    Ok(Self::init_reader(storage))
  }

  /// Get the reader and header using the key.
  async fn create_reader(&self, id: &str, format: &Format) -> Result<(Reader, Header)> {
    let mut reader = Self::reader(id, format, self.get_storage()).await?;

    let header = Self::read_raw_header(&mut reader)
      .await
      .map_err(|err| {
        HtsGetError::io_error(format!("Reading {} header: {}", self.get_format(), err))
      })?
      .parse::<Header>()
      .map_err(|_| HtsGetError::io_error(format!("Parsing {} header", self.get_format())))?;

    Ok((reader, header))
  }
}

/// The [BgzfSearch] trait defines commonalities for the formats that use a binning index, specifically
/// BAM, BCF, and VCF.
///
/// [S] is the storage type.
/// [ReaderType] is the inner type used for [Reader].
/// [ReferenceSequence] is the reference sequence type of the format's index.
/// [Index] is the format's index type.
/// [Reader] is the format's reader type.
/// [Header] is the format's header type.
#[async_trait]
pub(crate) trait BgzfSearch<S, ReaderType, ReferenceSequence, Index, Reader, Header>:
  Search<S, ReaderType, ReferenceSequence, Index, Reader, Header>
where
  S: AsyncStorage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
  Reader: BlockPosition + Send + Sync,
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
    query: Query,
    reference_sequence: &Self::ReferenceSequenceHeader,
    ref_seq_id: usize,
    index: &Index,
    seq_start: Option<i32>,
    seq_end: Option<i32>,
  ) -> Result<Vec<BytesRange>> {
    let seq_start = seq_start.unwrap_or(Self::MIN_SEQ_POSITION as i32);
    let seq_end = seq_end.unwrap_or_else(|| Self::max_seq_position(reference_sequence));
    let invalid_range = || HtsGetError::InvalidRange(format!("{}-{}", seq_start, seq_end));

    let seq_start = Position::try_from(seq_start as usize).map_err(|_| invalid_range())?;
    let seq_end = Position::try_from(seq_end as usize).map_err(|_| invalid_range())?;

    // TODO convert to async if supported later.
    let chunks = index
      .query(ref_seq_id, seq_start..=seq_end)
      .map_err(|_| invalid_range())?;

    let mut futures: FuturesUnordered<JoinHandle<Result<BytesRange>>> = FuturesUnordered::new();
    for chunk in merge_chunks(&chunks) {
      let storage = self.get_storage();
      let id = query.id.clone();
      let format = self.get_format();
      futures.push(tokio::spawn(async move {
        let mut reader = Self::reader(&id, &format, storage).await?;
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
    _id: &str,
    _format: &Format,
    _index: &Index,
  ) -> Result<Vec<BytesRange>> {
    Ok(Vec::new())
  }
}

#[async_trait]
impl<S, ReaderType, ReferenceSequence, Index, Reader, Header, T>
  SearchAll<S, ReaderType, ReferenceSequence, Index, Reader, Header> for T
where
  S: AsyncStorage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
  Reader: BlockPosition + Send + Sync,
  Header: FromStr + Send,
  ReferenceSequence: BinningIndexReferenceSequence + Sync,
  Index: BinningIndex<ReferenceSequence> + Send + Sync,
  T: BgzfSearch<S, ReaderType, ReferenceSequence, Index, Reader, Header> + Send + Sync,
{
  async fn get_byte_ranges_for_all(
    &self,
    id: String,
    format: Format,
    index: &Index,
  ) -> Result<Vec<BytesRange>> {
    let mut futures: FuturesUnordered<JoinHandle<Result<BytesRange>>> = FuturesUnordered::new();
    for ref_sequences in index.reference_sequences() {
      if let Some(metadata) = ref_sequences.metadata() {
        let storage = self.get_storage();
        let start_vpos = metadata.start_position();
        let end_vpos = metadata.end_position();
        let id = id.clone();
        futures.push(tokio::spawn(async move {
          let mut reader = Self::reader(&id, &format, storage).await?;
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

    let unmapped_byte_ranges = self
      .get_byte_ranges_for_unmapped(&id, &format, index)
      .await?;
    byte_ranges.extend(unmapped_byte_ranges.into_iter());
    Ok(BytesRange::merge_all(byte_ranges))
  }

  async fn get_byte_ranges_for_header(&self, query: &Query) -> Result<Vec<BytesRange>> {
    let (mut reader, _) = self.create_reader(&query.id, &self.get_format()).await?;
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
  async fn seek_vpos(&mut self, pos: VirtualPosition) -> io::Result<VirtualPosition>;
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
  /// where that block endS, which is not trivial nor possible for the last block.
  ///
  /// The solution used here goes through reading the records starting at the compressed
  /// virtual offset (coffset) of the end position (remember this will always be the
  /// start of a BGZF block).
  ///
  /// If we read the records pointed by that coffset until we reach a different coffset,
  /// we can find out where the current block ends. Therefore this can be used to only add the
  /// required bytes in the query results.
  ///
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
    reader.seek_vpos(*self).await.ok()?;
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
