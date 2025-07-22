//! The following file defines commonalities between all the file formats. While each format has
//! its own particularities, there are many shared components that can be abstracted.
//!
//! The generic types represent the specifics of the formats, and allow the abstractions to be made,
//! where the names of the types indicate their purpose.
//!

use std::collections::BTreeSet;

use async_trait::async_trait;
use futures::StreamExt;
use futures_util::stream::FuturesOrdered;
use noodles::bgzf::{gzi, VirtualPosition};
use noodles::csi::binning_index::index::reference_sequence::bin::Chunk;
use noodles::csi::binning_index::index::Index;
use noodles::csi::binning_index::index::{reference_sequence, ReferenceSequence};
use noodles::csi::binning_index::ReferenceSequence as ReferenceSequenceExt;
use noodles::csi::BinningIndex;
use tokio::io;
use tokio::io::{AsyncRead, BufReader};
use tokio::select;
use tokio::task::JoinHandle;
use tracing::{instrument, trace, trace_span, Instrument};

use htsget_config::types::Class::Header;

use crate::ConcurrencyError;
use crate::{Class, Class::Body, Format, HtsGetError, Query, Response, Result};
use htsget_storage::types::{
  BytesPosition, BytesPositionOptions, DataBlock, GetOptions, HeadOptions, RangeUrlOptions,
};
use htsget_storage::{Storage, StorageMiddleware, StorageTrait, Streamable};

// § 4.1.2 End-of-file marker <https://samtools.github.io/hts-specs/SAMv1.pdf>.
pub(crate) static BGZF_EOF: &[u8] = &[
  0x1f, 0x8b, 0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x06, 0x00, 0x42, 0x43, 0x02, 0x00,
  0x1b, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

pub(crate) const MAX_BGZF_ISIZE: u64 = 1 << 16;

/// Helper function to find the first non-none value from a set of futures.
pub(crate) async fn find_first<T>(
  msg: &str,
  mut futures: FuturesOrdered<JoinHandle<Option<T>>>,
) -> Result<T> {
  let mut result = None;
  loop {
    select! {
      Some(next) = futures.next() => {
        if let Some(next) = next.map_err(ConcurrencyError::new).map_err(HtsGetError::from)? {
          result = Some(next);
          break;
        }
      },
      else => break
    }
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
pub trait SearchAll<ReferenceSequence, Index, Reader, Header>
where
  Index: Send + Sync,
{
  /// This returns mapped and placed unmapped ranges.
  async fn get_byte_ranges_for_all(&self, query: &Query) -> Result<Vec<BytesPosition>>;

  /// Get the offset in the file of the end of the header.
  async fn get_header_end_offset(&self, index: &Index) -> Result<u64>;

  /// Returns the header bytes range.
  async fn get_byte_ranges_for_header(
    &self,
    index: &Index,
    reader: &mut Reader,
    query: &Query,
  ) -> Result<BytesPosition>;

  /// Get the eof marker for this format.
  fn get_eof_marker(&self) -> &[u8];

  /// Get the eof data block for this format.
  fn get_eof_data_block(&self) -> Option<DataBlock>;

  /// Get the eof bytes positions converting from a data block.
  fn get_eof_byte_positions(&self, file_size: u64) -> Option<Result<BytesPosition>> {
    if let Some(DataBlock::Data(data, class)) = self.get_eof_data_block() {
      let data_len =
        u64::try_from(data.len()).map_err(|err| HtsGetError::InvalidInput(err.to_string()));

      return match data_len {
        Ok(data_len) => {
          let bytes_position = BytesPosition::default()
            .with_start(file_size - data_len)
            .with_end(file_size);
          let bytes_position = bytes_position.set_class(class);

          Some(Ok(bytes_position))
        }
        Err(err) => Some(Err(err)),
      };
    }

    None
  }
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
pub trait SearchReads<ReferenceSequence, Index, Reader, Header>:
  Search<ReferenceSequence, Index, Reader, Header>
where
  Reader: Send,
  Header: Send + Sync,
  Index: Send + Sync,
{
  /// Get reference sequence from name.
  async fn get_reference_sequence_from_name<'b>(
    &self,
    header: &'b Header,
    name: &str,
  ) -> Option<usize>;

  /// Get unplaced unmapped ranges.
  async fn get_byte_ranges_for_unmapped_reads(
    &self,
    query: &Query,
    index: &Index,
  ) -> Result<Vec<BytesPosition>>;

  /// Get reads ranges for a reference sequence implementation.
  async fn get_byte_ranges_for_reference_sequence(
    &self,
    ref_seq_id: usize,
    query: &Query,
    index: &Index,
  ) -> Result<Vec<BytesPosition>>;

  ///Get reads for a given reference name and an optional sequence range.
  async fn get_byte_ranges_for_reference_name_reads(
    &self,
    reference_name: &str,
    index: &Index,
    header: &Header,
    query: &Query,
  ) -> Result<Vec<BytesPosition>> {
    if reference_name == "*" {
      return self.get_byte_ranges_for_unmapped_reads(query, index).await;
    }

    let maybe_ref_seq = self
      .get_reference_sequence_from_name(header, reference_name)
      .await;

    let byte_ranges = match maybe_ref_seq {
      None => Err(HtsGetError::not_found(format!(
        "reference name not found: {reference_name}"
      ))),
      Some(ref_seq_id) => {
        Self::get_byte_ranges_for_reference_sequence(self, ref_seq_id, query, index).await
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
pub trait Search<ReferenceSequence, Index, Reader, Header>:
  SearchAll<ReferenceSequence, Index, Reader, Header>
where
  Index: Send + Sync,
  Header: Send + Sync,
  Reader: Send,
  Self: Sync + Send,
{
  fn init_reader(inner: Streamable) -> Reader;
  async fn read_header(reader: &mut Reader) -> io::Result<Header>;
  async fn read_index_inner<T: AsyncRead + Unpin + Send>(inner: T) -> io::Result<Index>;

  /// Get ranges for a given reference name and an optional sequence range.
  async fn get_byte_ranges_for_reference_name(
    &self,
    reference_name: String,
    index: &Index,
    header: &Header,
    query: &Query,
  ) -> Result<Vec<BytesPosition>>;

  /// Get the storage of this format.
  fn get_storage(&self) -> &Storage;

  /// Get the mutable storage of this format.
  fn mut_storage(&mut self) -> &mut Storage;

  /// Get the format of this format.
  fn get_format(&self) -> Format;

  /// Get the position at the end of file marker.
  #[instrument(level = "trace", skip(self), ret)]
  async fn position_at_eof(&self, query: &Query) -> Result<u64> {
    let file_size = self.file_size(query).await?;
    Ok(
      file_size
        - u64::try_from(self.get_eof_marker().len())
          .map_err(|err| HtsGetError::InvalidInput(err.to_string()))?,
    )
  }

  /// Read the index from the key.
  #[instrument(level = "trace", skip(self))]
  async fn read_index(&self, query: &Query) -> Result<Index> {
    trace!("reading index");
    let storage = self
      .get_storage()
      .get(
        &query.format().fmt_index(query.id()),
        GetOptions::new_with_default_range(query.request().headers()),
      )
      .await?;
    Self::read_index_inner(storage)
      .await
      .map_err(|err| HtsGetError::io_error(format!("reading {} index: {}", self.get_format(), err)))
  }

  /// Search based on the query.
  async fn search(&mut self, query: Query) -> Result<Response> {
    match query.class() {
      Body => {
        let format = self.get_format();
        if format != query.format() {
          return Err(HtsGetError::unsupported_format(format!(
            "using `{}` search, but query contains `{}` format",
            format,
            query.format()
          )));
        }

        let index = self.read_index(&query).await?;
        let header_end = self.get_header_end_offset(&index).await?;

        self.preprocess(&query, header_end).await?;

        let mut byte_ranges = match query.reference_name().as_ref() {
          None => self.get_byte_ranges_for_all(&query).await?,
          Some(reference_name) => {
            let (header, mut reader) = self.get_header(&query, header_end).await?;

            let mut byte_ranges = self
              .get_byte_ranges_for_reference_name(
                reference_name.to_string(),
                &index,
                &header,
                &query,
              )
              .await?;

            byte_ranges.push(
              self
                .get_byte_ranges_for_header(&index, &mut reader, &query)
                .await?,
            );

            byte_ranges
          }
        };

        let file_size = self.file_size(&query).await?;
        if let Some(eof) = self.get_eof_byte_positions(file_size) {
          byte_ranges.push(eof?);
        }

        let blocks = self
          .get_storage()
          .postprocess(
            &query.format().fmt_file(query.id()),
            BytesPositionOptions::new(byte_ranges, query.request().headers()),
          )
          .await?;

        self.build_response(&query, blocks).await
      }
      Class::Header => {
        let index = self.read_index(&query).await?;
        let header_end = self.get_header_end_offset(&index).await?;

        self.preprocess(&query, header_end).await?;

        let (_, mut reader) = self.get_header(&query, header_end).await?;

        let header_byte_ranges = self
          .get_byte_ranges_for_header(&index, &mut reader, &query)
          .await?;

        let blocks = self
          .get_storage()
          .postprocess(
            &query.format().fmt_file(query.id()),
            BytesPositionOptions::new(vec![header_byte_ranges], query.request().headers()),
          )
          .await?;

        self.build_response(&query, blocks).await
      }
    }
  }

  async fn preprocess(&mut self, query: &Query, header_end: u64) -> Result<()> {
    Ok(
      self
        .mut_storage()
        .preprocess(
          &query.format().fmt_file(query.id()),
          GetOptions::new(
            BytesPosition::default().with_end(header_end),
            query.request().headers(),
          ),
        )
        .await?,
    )
  }

  async fn file_size(&self, query: &Query) -> Result<u64> {
    Ok(
      self
        .get_storage()
        .head(
          &query.format().fmt_file(query.id()),
          HeadOptions::new(query.request().headers()),
        )
        .await?,
    )
  }

  /// Build the response from the query using urls.
  #[instrument(level = "trace", skip(self, byte_ranges))]
  async fn build_response(&self, query: &Query, byte_ranges: Vec<DataBlock>) -> Result<Response> {
    trace!("building response");
    let mut urls = vec![];
    let storage = self.get_storage();

    for block in DataBlock::update_classes(byte_ranges) {
      match block {
        DataBlock::Range(range) => {
          trace!(range = ?range, "range");
          let query_owned = query.clone();

          urls.push(
            storage
              .range_url(
                &query_owned.format().fmt_file(query_owned.id()),
                RangeUrlOptions::new(range, query_owned.request().headers()),
              )
              .await?,
          );
        }
        DataBlock::Data(data, class) => {
          let data_url = self.get_storage().data_url(data, class);
          urls.push(data_url);
        }
      }
    }

    Ok(Response::new(query.format(), urls))
  }

  /// Get the header from the file specified by the id and format.
  #[instrument(level = "trace", skip(self))]
  async fn get_header(&self, query: &Query, offset: u64) -> Result<(Header, Reader)> {
    trace!("getting header");
    let get_options = GetOptions::new(
      BytesPosition::default().with_end(offset),
      query.request().headers(),
    );

    let reader_type = self
      .get_storage()
      .get(&query.format().fmt_file(query.id()), get_options)
      .await?;
    let mut reader = Self::init_reader(reader_type);

    Ok((
      Self::read_header(&mut reader).await.map_err(|err| {
        HtsGetError::io_error(format!("reading `{}` header: {}", self.get_format(), err))
      })?,
      reader,
    ))
  }
}

/// The [BgzfSearch] trait defines commonalities for the formats that use a binning index, specifically
/// BAM, BCF, and VCF.
///
/// [S] is the storage type.
/// [I] the index type used for the `ReferenceSequence`.
/// [ReaderType] is the inner type used for [Reader].
/// [ReferenceSequence] is the reference sequence type of the format's index.
/// [Index] is the format's index type.
/// [Reader] is the format's reader type.
/// [Header] is the format's header type.
#[async_trait]
pub trait BgzfSearch<I, Reader, Header>:
  Search<ReferenceSequence<I>, Index<I>, Reader, Header>
where
  I: reference_sequence::Index + Send + Sync,
  Reader: Send + Sync,
  Header: Send + Sync,
{
  #[instrument(level = "trace", skip_all)]
  fn index_positions(index: &Index<I>) -> BTreeSet<u64> {
    trace!("getting possible index positions");
    let mut positions = BTreeSet::new();

    // Its probably most robust to search through all chunks in all reference sequences.
    // See https://github.com/samtools/htslib/issues/1482
    positions.extend(
      index
        .reference_sequences()
        .iter()
        .flat_map(|ref_seq| ref_seq.bins())
        .flat_map(|(_, bin)| bin.chunks())
        .flat_map(|chunk| [chunk.start().compressed(), chunk.end().compressed()]),
    );

    positions.extend(
      index
        .reference_sequences()
        .iter()
        .filter_map(|ref_seq| ref_seq.metadata())
        .flat_map(|metadata| {
          [
            metadata.start_position().compressed(),
            metadata.end_position().compressed(),
          ]
        }),
    );

    positions
  }

  /// Get ranges for a reference sequence for the bgzf format.
  #[instrument(level = "trace", skip_all)]
  async fn get_byte_ranges_for_reference_sequence_bgzf(
    &self,
    query: &Query,
    ref_seq_id: usize,
    index: &Index<I>,
  ) -> Result<Vec<BytesPosition>> {
    let chunks: Result<Vec<Chunk>> = trace_span!("querying chunks").in_scope(|| {
      trace!(id = ?query.id(), ref_seq_id = ?ref_seq_id, "querying chunks");
      let mut chunks = index
        .query(ref_seq_id, query.interval().into_one_based()?)
        .map_err(|err| HtsGetError::InvalidRange(format!("querying range: {err}")))?;

      chunks.sort_unstable_by_key(|a| a.end().compressed());

      Ok(chunks)
    });

    let gzi_data = self
      .get_storage()
      .get(
        &query.format().fmt_gzi(query.id())?,
        GetOptions::new_with_default_range(query.request().headers()),
      )
      .await;
    let byte_ranges: Vec<BytesPosition> = match gzi_data {
      Ok(gzi_data) => {
        let span = trace_span!("reading gzi");
        let gzi: Result<Vec<u64>> = async {
          trace!(id = ?query.id(), "reading gzi");
          let gzi_index = gzi::r#async::io::Reader::new(BufReader::new(gzi_data))
            .read_index()
            .await?;
          let mut gzi: Vec<u64> = gzi_index
            .as_ref()
            .iter()
            .map(|(compressed, _)| *compressed)
            .collect();

          trace!(id = ?query.id(), "sorting gzi");
          gzi.sort_unstable();
          Ok(gzi)
        }
        .instrument(span)
        .await;

        self
          .bytes_positions_from_chunks(query, chunks?.into_iter(), gzi?.into_iter())
          .await?
      }
      Err(_) => {
        self
          .bytes_positions_from_chunks(
            query,
            chunks?.into_iter(),
            Self::index_positions(index).into_iter(),
          )
          .await?
      }
    };

    Ok(byte_ranges)
  }

  /// Assumes sorted chunks by compressed end position, and sorted positions.
  #[instrument(level = "trace", skip(self, chunks, positions))]
  async fn bytes_positions_from_chunks<'a>(
    &self,
    query: &Query,
    chunks: impl Iterator<Item = Chunk> + Send + 'a,
    mut positions: impl Iterator<Item = u64> + Send + 'a,
  ) -> Result<Vec<BytesPosition>> {
    trace!("processing index and chunks");

    let mut end_position: Option<u64> = None;
    let mut bytes_positions = Vec::new();
    let mut maybe_end: Option<u64> = None;

    let mut append_position = |chunk: Chunk, end: u64| {
      bytes_positions.push(
        BytesPosition::default()
          .with_start(chunk.start().compressed())
          .with_end(end)
          .with_class(Body),
      );
    };

    for chunk in chunks {
      match maybe_end {
        Some(pos) if pos > chunk.end().compressed() => {
          append_position(chunk, pos);
          continue;
        }
        _ => {}
      }

      maybe_end = positions.find(|pos| pos > &chunk.end().compressed());

      let end = match maybe_end {
        None => match end_position {
          None => {
            let pos = self.position_at_eof(query).await?;
            end_position = Some(pos);
            pos
          }
          Some(pos) => pos,
        },
        Some(pos) => pos,
      };

      append_position(chunk, end);
    }

    Ok(bytes_positions)
  }

  /// Get unmapped bytes ranges.
  async fn get_byte_ranges_for_unmapped(
    &self,
    _query: &Query,
    _index: &Index<I>,
  ) -> Result<Vec<BytesPosition>> {
    Ok(Vec::new())
  }

  /// Get the virtual position of the underlying reader.
  async fn read_bytes(reader: &mut Reader) -> Option<usize>;

  /// Get the virtual position of the underlying reader.
  fn virtual_position(&self, reader: &Reader) -> VirtualPosition;
}

#[async_trait]
impl<I, Reader, Header, T> SearchAll<ReferenceSequence<I>, Index<I>, Reader, Header> for T
where
  I: reference_sequence::Index + Send + Sync,
  Reader: Send + Sync,
  Header: Send + Sync,
  T: BgzfSearch<I, Reader, Header> + Send + Sync,
{
  #[instrument(level = "debug", skip(self), ret)]
  async fn get_byte_ranges_for_all(&self, query: &Query) -> Result<Vec<BytesPosition>> {
    Ok(vec![
      BytesPosition::default().with_end(self.position_at_eof(query).await?)
    ])
  }

  #[instrument(level = "trace", skip_all, ret)]
  async fn get_header_end_offset(&self, index: &Index<I>) -> Result<u64> {
    let first_index_position =
      Self::index_positions(index)
        .into_iter()
        .next()
        .ok_or_else(|| {
          HtsGetError::io_error(format!(
            "finding header offset in `{}` index",
            self.get_format()
          ))
        })?;

    // The header can only extend past the first index position by the maximum BGZF block size
    // because otherwise the first index position wouldn't be representing the first reference.
    Ok(first_index_position + MAX_BGZF_ISIZE)
  }

  async fn get_byte_ranges_for_header(
    &self,
    index: &Index<I>,
    reader: &mut Reader,
    query: &Query,
  ) -> Result<BytesPosition> {
    let current_block_index = self.virtual_position(reader);

    let mut next_block_index = if current_block_index.uncompressed() == 0 {
      current_block_index.compressed()
    } else {
      loop {
        let bytes_read = Self::read_bytes(reader).await.unwrap_or_default();
        let actual_block_index = self.virtual_position(reader).compressed();

        if bytes_read == 0 || actual_block_index > current_block_index.compressed() {
          break actual_block_index;
        }
      }
    };

    next_block_index = if next_block_index == 0 {
      // if for some reason that fails, get the second position from the index.
      let mut positions = Self::index_positions(index);

      positions.pop_first();

      let position = positions.into_iter().next().unwrap_or_default();

      if position == 0 {
        self.position_at_eof(query).await?
      } else {
        position
      }
    } else {
      next_block_index
    };

    Ok(
      BytesPosition::default()
        .with_start(0)
        .with_end(next_block_index)
        .with_class(Header),
    )
  }

  fn get_eof_marker(&self) -> &[u8] {
    BGZF_EOF
  }

  fn get_eof_data_block(&self) -> Option<DataBlock> {
    Some(DataBlock::Data(Vec::from(BGZF_EOF), Some(Body)))
  }
}
