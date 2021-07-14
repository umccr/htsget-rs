//! The following file defines commonalities between all the file formats. While each format has
//! its own particularities, there are many shared components that can be abstracted.
//!
//! The generic types represent the specifics of the formats, and allow the abstractions to be made,
//! where the names of the types indicate their purpose.
//!

use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::str::FromStr;

use noodles_bgzf::VirtualPosition;
use noodles_csi::binning_index::merge_chunks;
use noodles_csi::{BinningIndex, BinningIndexReferenceSequence};
use noodles_sam as sam;

use crate::storage::GetOptions;
use crate::{
  htsget::{Class, Format, HtsGetError, Query, Response, Result, Url},
  storage::{BytesRange, Storage, UrlOptions},
};

/// [SearchAll] represents searching bytes ranges that are applicable to all formats. Specifically,
/// range for the whole file, and the header.
///
/// [ReferenceSequence] is the reference sequence type of the format's index.
/// [Index] is the format's index type.
/// [Reader] is the format's reader type.
/// [Header] is the format's header type.
pub(crate) trait SearchAll<'a, S, ReferenceSequence, Index, Reader, Header> {
  /// This returns mapped and placed unmapped ranges.
  fn get_byte_ranges_for_all(&self, key: &str, index: &Index) -> Result<Vec<BytesRange>>;

  /// Returns the header bytes range.
  fn get_byte_ranges_for_header(&self, key: &str) -> Result<Vec<BytesRange>>;
}

/// [SearchReads] represents searching bytes ranges for the reads endpoint.
///
/// [ReferenceSequence] is the reference sequence type of the format's index.
/// [Index] is the format's index type.
/// [Reader] is the format's reader type.
/// [Header] is the format's header type.
pub(crate) trait SearchReads<'a, S, ReferenceSequence, Index, Reader, Header>:
  Search<'a, S, ReferenceSequence, Index, Reader, Header>
where
  S: Storage + 'a,
  Header: FromStr,
{
  /// Get reference sequence from name.
  fn get_reference_sequence_from_name<'b>(
    &self,
    header: &'b Header,
    name: &str,
  ) -> Option<(usize, &'b String, &'b sam::header::ReferenceSequence)>;

  /// Get unplaced unmapped ranges.
  fn get_byte_ranges_for_unmapped_reads(&self, key: &str, index: &Index)
    -> Result<Vec<BytesRange>>;

  /// Get reads ranges for a reference sequence implementation.
  fn get_byte_ranges_for_reference_sequence(
    &self,
    key: &str,
    reference_sequence: &sam::header::ReferenceSequence,
    ref_seq_id: usize,
    query: &Query,
    index: &Index,
    reader: &mut Reader,
  ) -> Result<Vec<BytesRange>>;

  ///Get reads for a given reference name and an optional sequence range.
  fn get_byte_ranges_for_reference_name_reads(
    &self,
    key: &str,
    reference_name: &str,
    index: &Index,
    query: &Query,
  ) -> Result<Vec<BytesRange>> {
    if reference_name == "*" {
      return self.get_byte_ranges_for_unmapped_reads(key, &index);
    }

    let (mut reader, header) = self.create_reader(key)?;
    let maybe_ref_seq = self.get_reference_sequence_from_name(&header, reference_name);

    let byte_ranges = match maybe_ref_seq {
      None => Err(HtsGetError::not_found(format!(
        "Reference name not found: {}",
        reference_name
      ))),
      Some((bam_ref_seq_idx, _, bam_ref_seq)) => Self::get_byte_ranges_for_reference_sequence(
        self,
        key,
        bam_ref_seq,
        bam_ref_seq_idx,
        query,
        index,
        &mut reader,
      ),
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
pub(crate) trait Search<'a, S, ReferenceSequence, Index, Reader, Header>:
  SearchAll<'a, S, ReferenceSequence, Index, Reader, Header>
where
  S: Storage + 'a,
  Header: FromStr,
{
  const MIN_SEQ_POSITION: u32 = 1; // 1-based

  const READER_FN: fn(File) -> Reader;
  const HEADER_FN: fn(&mut Reader) -> io::Result<String>;
  const INDEX_FN: fn(PathBuf) -> io::Result<Index>;

  /// Get ranges for a given reference name and an optional sequence range.
  fn get_byte_ranges_for_reference_name(
    &self,
    key: &str,
    reference_name: &str,
    index: &Index,
    query: &Query,
  ) -> Result<Vec<BytesRange>>;

  /// Generate a key for the storage object from an ID
  /// This may involve a more complex transformation in the future,
  /// or even require custom implementations depending on the organizational structure
  /// For now there is a 1:1 mapping to the underlying files
  fn get_keys_from_id(&self, id: &str) -> (String, String);

  /// Get the storage of this trait.
  fn get_storage(&self) -> &S;

  /// Get the format of this trait.
  fn get_format(&self) -> Format;

  /// Read the index from the key.
  fn read_index(&self, key: &str) -> Result<Index> {
    let path = self.get_storage().get(&key, GetOptions::default())?;
    Self::INDEX_FN(path)
      .map_err(|_| HtsGetError::io_error(format!("Reading {} index file", self.get_format())))
  }

  /// Search based on the query.
  fn search(&self, query: Query) -> Result<Response> {
    let (file_key, index_key) = self.get_keys_from_id(query.id.as_str());

    match query.class {
      Class::Body => {
        let index = self.read_index(&index_key)?;

        let byte_ranges = match query.reference_name.as_ref() {
          None => self.get_byte_ranges_for_all(file_key.as_str(), &index)?,
          Some(reference_name) => self.get_byte_ranges_for_reference_name(
            file_key.as_str(),
            reference_name,
            &index,
            &query,
          )?,
        };
        self.build_response(query, &file_key, byte_ranges)
      }
      Class::Header => {
        let byte_ranges = self.get_byte_ranges_for_header(&file_key)?;
        self.build_response(query, &file_key, byte_ranges)
      }
    }
  }

  /// Build the response from the query using urls.
  fn build_response(
    &self,
    query: Query,
    key: &str,
    byte_ranges: Vec<BytesRange>,
  ) -> Result<Response> {
    let urls = byte_ranges
      .into_iter()
      .map(|range| {
        let options = UrlOptions::default()
          .with_range(range)
          .with_class(query.class.clone());
        self
          .get_storage()
          .url(&key, options)
          .map_err(HtsGetError::from)
      })
      .collect::<Result<Vec<Url>>>()?;

    let format = query.format.unwrap_or_else(|| self.get_format());
    Ok(Response::new(format, urls))
  }

  /// Get the reader from the key.
  fn get_reader<U: Into<String>>(&self, key: &str, msg: U) -> Result<Reader> {
    let get_options = GetOptions::default();
    let path = self.get_storage().get(key, get_options)?;

    File::open(path)
      .map(Self::READER_FN)
      .map_err(|_| HtsGetError::io_error(msg))
  }

  /// Get the reader and header using the key.
  fn create_reader(&self, key: &str) -> Result<(Reader, Header)> {
    let mut reader = self.get_reader(key, format!("Reading {}", self.get_format()))?;

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
pub(crate) trait BgzfSearch<'a, S, ReferenceSequence, Index, Reader, Header>:
  Search<'a, S, ReferenceSequence, Index, Reader, Header>
where
  S: Storage + 'a,
  Reader: BlockPosition,
  ReferenceSequence: 'a + BinningIndexReferenceSequence,
  Index: BinningIndex<ReferenceSequence>,
  Header: FromStr,
{
  type ReferenceSequenceHeader;

  /// Get the max sequence position.
  fn max_seq_position(ref_seq: &Self::ReferenceSequenceHeader) -> i32;

  /// Get ranges for a reference sequence for the bgzf format.
  fn get_byte_ranges_for_reference_sequence_bgzf(
    &self,
    reference_sequence: &Self::ReferenceSequenceHeader,
    ref_seq_id: usize,
    index: &'a Index,
    seq_start: Option<i32>,
    seq_end: Option<i32>,
    reader: &mut Reader,
  ) -> Result<Vec<BytesRange>> {
    let seq_start = seq_start.unwrap_or(Self::MIN_SEQ_POSITION as i32);
    let seq_end = seq_end.unwrap_or_else(|| Self::max_seq_position(reference_sequence));

    let chunks = index
      .query(ref_seq_id, seq_start..=seq_end)
      .map_err(|_| HtsGetError::InvalidRange(format!("{}-{}", seq_start, seq_end)))?;

    let byte_ranges = merge_chunks(&chunks)
      .into_iter()
      .map(|chunk| {
        BytesRange::default()
          .with_start(chunk.start().bytes_range_start())
          .with_end(chunk.end().bytes_range_end(reader))
      })
      .collect();

    Ok(BytesRange::merge_all(byte_ranges))
  }

  /// Get unmapped bytes ranges.
  fn get_byte_ranges_for_unmapped(&self, _key: &str, _index: &Index) -> Result<Vec<BytesRange>> {
    Ok(Vec::new())
  }
}

impl<'a, S, ReferenceSequence, Index, Reader, Header, T>
  SearchAll<'a, S, ReferenceSequence, Index, Reader, Header> for T
where
  S: Storage + 'a,
  Reader: BlockPosition,
  Header: FromStr,
  ReferenceSequence: 'a + BinningIndexReferenceSequence,
  Index: BinningIndex<ReferenceSequence>,
  T: BgzfSearch<'a, S, ReferenceSequence, Index, Reader, Header>,
{
  fn get_byte_ranges_for_all(&self, key: &str, index: &Index) -> Result<Vec<BytesRange>> {
    let (mut reader, _) = self.create_reader(key)?;

    let mut byte_ranges: Vec<BytesRange> = Vec::new();
    for ref_sequences in index.reference_sequences() {
      if let Some(metadata) = ref_sequences.metadata() {
        let start_vpos = metadata.start_position();
        let end_vpos = metadata.end_position();
        byte_ranges.push(
          BytesRange::default()
            .with_start(start_vpos.bytes_range_start())
            .with_end(end_vpos.bytes_range_end(&mut reader)),
        );
      }
    }

    let unmapped_byte_ranges = self.get_byte_ranges_for_unmapped(key, index)?;
    byte_ranges.extend(unmapped_byte_ranges.into_iter());
    Ok(BytesRange::merge_all(byte_ranges))
  }

  fn get_byte_ranges_for_header(&self, key: &str) -> Result<Vec<BytesRange>> {
    let (mut reader, _) = self.create_reader(key)?;
    Ok(vec![BytesRange::default().with_start(0).with_end(
      reader.virtual_position().bytes_range_end(&mut reader),
    )])
  }
}

/// A block position extends the concept of a virtual position for readers.
pub(crate) trait BlockPosition {
  /// Read bytes of record.
  fn read_bytes(&mut self) -> Option<usize>;
  /// Seek using VirtualPosition.
  fn seek(&mut self, pos: VirtualPosition) -> io::Result<VirtualPosition>;
  /// Read the virtual position.
  fn virtual_position(&self) -> VirtualPosition;
}

/// An extension trait for VirtualPosition, which defines some common functions for the Bgzf formats.
pub(crate) trait VirtualPositionExt {
  const MAX_BLOCK_SIZE: u64 = 65536;

  /// Get the starting bytes for a compressed BGZF block.
  fn bytes_range_start(&self) -> u64;
  /// Get the ending bytes for a compressed BGZF block.
  fn bytes_range_end(&self, reader: &mut dyn BlockPosition) -> u64;
  fn to_string(&self) -> String;
  fn get_next_block_position(&self, reader: &mut dyn BlockPosition) -> Option<u64>;
}

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
  fn bytes_range_end(&self, reader: &mut dyn BlockPosition) -> u64 {
    if self.uncompressed() == 0 {
      // If the uncompressed part is exactly zero, we don't need the next block
      return self.compressed();
    }
    self
      .get_next_block_position(reader)
      .unwrap_or(self.compressed() + Self::MAX_BLOCK_SIZE)
  }

  /// Convert to string.
  fn to_string(&self) -> String {
    format!("{}/{}", self.compressed(), self.uncompressed())
  }

  /// Get the next block position from the reader.
  fn get_next_block_position(&self, reader: &mut dyn BlockPosition) -> Option<u64> {
    reader.seek(*self).ok()?;
    let next_block_index = loop {
      let bytes_read = reader.read_bytes()?;
      let actual_block_index = reader.virtual_position().compressed();
      if bytes_read == 0 || actual_block_index > self.compressed() {
        break actual_block_index;
      }
    };
    Some(next_block_index)
  }
}
