//! The following file defines commonalities between all the file formats. While each format has
//! its own particularities, there are many shared components that can be abstracted.
//!
//! The generic types represent the specifics of the formats, and allow the abstractions to be made,
//! where the names of the types indicate their purpose.
//!

use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use futures_util::stream::FuturesOrdered;
use noodles::bgzf::VirtualPosition;
use noodles::core::Position;
use noodles::csi::binning_index::merge_chunks;
use noodles::csi::{BinningIndex, BinningIndexReferenceSequence};
use noodles::sam;
use tokio::io;
use tokio::io::AsyncRead;
use tokio::select;
use tokio::task::JoinHandle;

use crate::storage::{DataBlock, GetOptions};
use crate::{
  htsget::{Class, Format, HtsGetError, Query, Response, Result},
  storage::{BytesPosition, RangeUrlOptions, Storage},
};

// § 4.1.2 End-of-file marker <https://samtools.github.io/hts-specs/SAMv1.pdf>.
pub(crate) static BGZF_EOF: &[u8] = &[
  0x1f, 0x8b, 0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x06, 0x00, 0x42, 0x43, 0x02, 0x00,
  0x1b, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Helper function to find the first non-none value from a set of futures.
pub(crate) async fn find_first<T>(
  msg: &str,
  mut futures: FuturesOrdered<JoinHandle<Option<T>>>,
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

/// Helper function to convert a 0-based position to a 1-based position.
pub(crate) fn into_one_based_position(value: i32) -> Result<i32> {
  value.checked_add(1).ok_or_else(|| {
    HtsGetError::InvalidRange(format!("Could not convert {} to 1-based position.", value))
  })
}

/// [SearchEof] handles data blocks that specify the end of the file for formats.
///
/// [S] is the storage type.
/// [ReaderType] is the inner type used for [Reader].
/// [ReferenceSequence] is the reference sequence type of the format's index.
/// [Index] is the format's index type.
/// [Reader] is the format's reader type.
/// [Header] is the format's header type.
pub(crate) trait SearchEof<S, ReaderType, ReferenceSequence, Index, Reader, Header> {
  /// Get the eof marker for this format. Defaults to BGZF eof.
  fn get_eof_marker(&self) -> Option<DataBlock>;
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
pub(crate) trait SearchAll<S, ReaderType, ReferenceSequence, Index, Reader, Header>:
  SearchEof<S, ReaderType, ReferenceSequence, Index, Reader, Header>
where
  Index: Send + Sync,
{
  /// This returns mapped and placed unmapped ranges.
  async fn get_byte_ranges_for_all(
    &self,
    id: String,
    format: Format,
    index: &Index,
  ) -> Result<Vec<BytesPosition>>;

  /// Returns the header bytes range.
  async fn get_byte_ranges_for_header(&self, query: &Query) -> Result<Vec<BytesPosition>>;

  /// Get the offset in the file of the end of the header.
  async fn get_header_end_offset(&self, index: &Index) -> Result<u64>;
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
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
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
  ) -> Result<Vec<BytesPosition>>;

  /// Get reads ranges for a reference sequence implementation.
  async fn get_byte_ranges_for_reference_sequence(
    &self,
    reference_sequence: &sam::header::ReferenceSequence,
    ref_seq_id: usize,
    query: Query,
    index: &Index,
  ) -> Result<Vec<BytesPosition>>;

  ///Get reads for a given reference name and an optional sequence range.
  async fn get_byte_ranges_for_reference_name_reads(
    &self,
    reference_name: &str,
    index: &Index,
    query: Query,
  ) -> Result<Vec<BytesPosition>> {
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
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
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
  ) -> Result<Vec<BytesPosition>>;

  /// Get the storage of this format.
  fn get_storage(&self) -> Arc<S>;

  /// Get the format of this format.
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
    let mut header_byte_ranges = self.get_byte_ranges_for_header(&query).await?;

    match query.class {
      Class::Body => {
        let index = self.read_index(&query.id).await?;

        let format = self.get_format();
        if format != query.format {
          return Err(HtsGetError::unsupported_format(format!(
            "Using {} search, but query contains {} format.",
            format, query.format
          )));
        }

        let id = query.id.clone();
        let class = query.class.clone();
        let mut byte_ranges = match query.reference_name.as_ref() {
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

        header_byte_ranges.append(&mut byte_ranges);
        let mut blocks =
          DataBlock::from_bytes_positions(BytesPosition::merge_all(header_byte_ranges));
        if let Some(eof) = self.get_eof_marker() {
          blocks.push(eof);
        }

        self.build_response(class, id, format, blocks).await
      }
      Class::Header => {
        self
          .build_response(
            query.class,
            query.id,
            self.get_format(),
            DataBlock::from_bytes_positions(header_byte_ranges),
          )
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
    byte_ranges: Vec<DataBlock>,
  ) -> Result<Response> {
    let mut storage_futures = FuturesOrdered::new();
    for block in byte_ranges {
      match block {
        DataBlock::Range(range) => {
          let options = RangeUrlOptions::default()
            .with_range(range)
            .with_class(class.clone());
          let storage = self.get_storage();
          let id = id.clone();
          storage_futures.push(tokio::spawn(async move {
            storage.range_url(format.fmt_file(&id), options).await
          }));
        }
        DataBlock::Data(data) => {
          let class_copy = class.clone();
          storage_futures.push(tokio::spawn(
            async move { Ok(S::data_url(data, class_copy)) },
          ));
        }
      }
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
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
  Reader: BlockPosition + Send + Sync,
  ReferenceSequence: BinningIndexReferenceSequence,
  Index: BinningIndex + Send + Sync,
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
  ) -> Result<Vec<BytesPosition>> {
    let seq_start = seq_start.map(into_one_based_position).transpose()?;
    let seq_end = seq_end.map(into_one_based_position).transpose()?;

    let seq_start = seq_start.unwrap_or(Self::MIN_SEQ_POSITION as i32);
    let seq_end = seq_end.unwrap_or_else(|| Self::max_seq_position(reference_sequence));
    let invalid_range = || HtsGetError::InvalidRange(format!("{}-{}", seq_start, seq_end));

    let seq_start = Position::try_from(seq_start as usize).map_err(|_| invalid_range())?;
    let seq_end = Position::try_from(seq_end as usize).map_err(|_| invalid_range())?;

    let chunks = index
      .query(ref_seq_id, seq_start..=seq_end)
      .map_err(|_| invalid_range())?;

    let mut futures: FuturesOrdered<JoinHandle<Result<BytesPosition>>> = FuturesOrdered::new();
    for chunk in merge_chunks(&chunks) {
      let storage = self.get_storage();
      let id = query.id.clone();
      let format = self.get_format();
      futures.push(tokio::spawn(async move {
        let mut reader = Self::reader(&id, &format, storage).await?;
        Ok(
          BytesPosition::default()
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

    Ok(BytesPosition::merge_all(byte_ranges))
  }

  /// Get unmapped bytes ranges.
  async fn get_byte_ranges_for_unmapped(
    &self,
    _id: &str,
    _format: &Format,
    _index: &Index,
  ) -> Result<Vec<BytesPosition>> {
    Ok(Vec::new())
  }
}

#[async_trait]
impl<S, ReaderType, ReferenceSequence, Index, Reader, Header, T>
  SearchAll<S, ReaderType, ReferenceSequence, Index, Reader, Header> for T
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
  Reader: BlockPosition + Send + Sync,
  Header: FromStr + Send,
  ReferenceSequence: BinningIndexReferenceSequence + Sync,
  Index: BinningIndex + Send + Sync,
  T: BgzfSearch<S, ReaderType, ReferenceSequence, Index, Reader, Header> + Send + Sync,
{
  async fn get_byte_ranges_for_all(
    &self,
    id: String,
    format: Format,
    _index: &Index,
  ) -> Result<Vec<BytesPosition>> {
    let file_size = self
      .get_storage()
      .head(format.fmt_file(&id))
      .await
      .map_err(|_| HtsGetError::io_error("Reading file size"))?;

    Ok(vec![BytesPosition::default()
      .with_start(0)
      .with_end(file_size - BGZF_EOF.len() as u64)])
  }

  async fn get_byte_ranges_for_header(&self, query: &Query) -> Result<Vec<BytesPosition>> {
    let (mut reader, _) = self.create_reader(&query.id, &self.get_format()).await?;
    let virtual_position = reader.virtual_position();
    Ok(vec![BytesPosition::default().with_start(0).with_end(
      virtual_position.bytes_range_end(&mut reader).await,
    )])
  }

  async fn get_header_end_offset(&self, index: &Index) -> Result<u64> {
    let chunks = index.query(0, ..)?;
    // Do we have to search all chunks? Can we assume the first chunk contains the start ref_seq?
    chunks.iter().map(|chunk| chunk.start().compressed()).min().ok_or_else(|| {
      HtsGetError::io_error("No chunks found in index")
    })
  }
}

impl<S, ReaderType, ReferenceSequence, Index, Reader, Header, T>
  SearchEof<S, ReaderType, ReferenceSequence, Index, Reader, Header> for T
where
  S: Storage<Streamable = ReaderType> + Send + Sync + 'static,
  ReaderType: AsyncRead + Unpin + Send + Sync,
  Reader: BlockPosition + Send + Sync,
  Header: FromStr + Send,
  ReferenceSequence: BinningIndexReferenceSequence + Sync,
  Index: BinningIndex + Send + Sync,
  T: BgzfSearch<S, ReaderType, ReferenceSequence, Index, Reader, Header> + Send + Sync,
{
  fn get_eof_marker(&self) -> Option<DataBlock> {
    Some(DataBlock::Data(Vec::from(BGZF_EOF)))
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
  /// where that block ends, which is not trivial nor possible for the last block.
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
