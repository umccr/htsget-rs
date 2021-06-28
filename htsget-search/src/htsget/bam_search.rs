//! Module providing the search capability using BAM/BAI files
//!

use std::{fs::File, path::Path};

use noodles_bam::{self as bam, bai};
use noodles_bgzf::index::{optimize_chunks, Chunk};
use noodles_bgzf::VirtualPosition;
use noodles_sam::{self as sam};

use crate::{
  htsget::{Class, Format, HtsGetError, Query, Response, Result, Url},
  storage::{BytesRange, GetOptions, Storage, UrlOptions},
};

trait VirtualPositionExt {
  const MAX_BLOCK_SIZE: u64 = 65536;

  /// Get the starting bytes for a compressed BGZF block.
  fn bytes_range_start(&self) -> u64;
  /// Get the ending bytes for a compressed BGZF block.
  fn bytes_range_end(&self, reader: &mut bam::Reader<File>) -> u64;
  fn to_string(&self) -> String;
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
  fn bytes_range_end(&self, reader: &mut bam::Reader<File>) -> u64 {
    if self.uncompressed() == 0 {
      // If the uncompressed part is exactly zero, we don't need the next block
      return self.compressed();
    }
    get_next_block_position(*self, reader).unwrap_or(self.compressed() + Self::MAX_BLOCK_SIZE)
  }

  fn to_string(&self) -> String {
    format!("{}/{}", self.compressed(), self.uncompressed())
  }
}

fn get_next_block_position(
  block_position: VirtualPosition,
  reader: &mut bam::Reader<File>,
) -> Option<u64> {
  reader.seek(block_position).ok()?;
  let next_block_index = loop {
    let mut record = bam::Record::default();
    let bytes_read = reader.read_record(&mut record).ok()?;
    let actual_block_index = reader.virtual_position().compressed();
    if bytes_read == 0 || actual_block_index > block_position.compressed() {
      break actual_block_index;
    }
  };
  Some(next_block_index)
}

pub(crate) struct BamSearch<'a, S> {
  storage: &'a S,
}

impl<'a, S> BamSearch<'a, S>
where
  S: Storage + 'a,
{
  /// 1 Mb
  const DEFAULT_BAM_HEADER_LENGTH: u64 = 1024 * 1024; // TODO find a number that makes more sense

  const MIN_SEQ_POSITION: u32 = 1; // 1-based

  pub fn new(storage: &'a S) -> Self {
    Self { storage }
  }

  pub fn search(&self, query: Query) -> Result<Response> {
    let (bam_key, bai_key) = self.get_keys_from_id(query.id.as_str());

    match query.class {
      Class::Body => {
        let bai_path = self.storage.get(&bai_key, GetOptions::default())?;
        let bai_index = bai::read(bai_path).map_err(|_| HtsGetError::io_error("Reading BAI"))?;

        let byte_ranges = match query.reference_name.as_ref() {
          None => self.get_byte_ranges_for_all_reads(bam_key.as_str(), &bai_index)?,
          Some(reference_name) if reference_name.as_str() == "*" => {
            self.get_byte_ranges_for_unmapped_reads(bam_key.as_str(), &bai_index)?
          }
          Some(reference_name) => self.get_byte_ranges_for_reference_name(
            bam_key.as_str(),
            reference_name,
            &bai_index,
            &query,
          )?,
        };
        self.build_response(query, &bam_key, byte_ranges)
      }
      Class::Header => {
        let byte_ranges = self.get_byte_ranges_for_header();
        self.build_response(query, &bam_key, byte_ranges)
      }
    }
  }

  /// Generate a key for the storage object from an ID
  /// This may involve a more complex transformation in the future,
  /// or even require custom implementations depending on the organizational structure
  /// For now there is a 1:1 mapping to the underlying files
  fn get_keys_from_id(&self, id: &str) -> (String, String) {
    let bam_key = format!("{}.bam", id);
    let bai_key = format!("{}.bai", bam_key);
    (bam_key, bai_key)
  }

  /// This returns mapped and placed unmapped ranges
  fn get_byte_ranges_for_all_reads(
    &self,
    bam_key: &str,
    bai_index: &bai::Index,
  ) -> Result<Vec<BytesRange>> {
    let mut byte_ranges: Vec<BytesRange> = Vec::new();
    let get_options = GetOptions::default().with_max_length(Self::DEFAULT_BAM_HEADER_LENGTH);
    let bam_path = self.storage.get(bam_key, get_options)?;
    let mut bam_reader = Self::get_bam_reader(bam_path)?;
    for reference_sequence in bai_index.reference_sequences() {
      if let Some(metadata) = reference_sequence.metadata() {
        let start_vpos = metadata.start_position();
        let end_vpos = metadata.end_position();
        byte_ranges.push(
          BytesRange::default()
            .with_start(start_vpos.bytes_range_start())
            .with_end(end_vpos.bytes_range_end(&mut bam_reader)),
        );
      }
    }

    let unmapped_byte_ranges = self.get_byte_ranges_for_unmapped_reads(bam_key, bai_index)?;
    byte_ranges.extend(unmapped_byte_ranges.into_iter());
    Ok(BytesRange::merge_all(byte_ranges))
  }

  /// This returns only unplaced unmapped ranges
  fn get_byte_ranges_for_unmapped_reads(
    &self,
    bam_key: &str,
    bai_index: &bai::Index,
  ) -> Result<Vec<BytesRange>> {
    let last_interval = bai_index
      .reference_sequences()
      .iter()
      .rev()
      .find_map(|rs| rs.intervals().last().cloned());

    let start = match last_interval {
      Some(start) => start,
      None => {
        let get_options = GetOptions::default().with_max_length(Self::DEFAULT_BAM_HEADER_LENGTH);
        let bam_path = self.storage.get(bam_key, get_options)?;
        let (bam_reader, _) = Self::read_bam_header(&bam_path)?;
        bam_reader.virtual_position()
      }
    };

    // TODO get the end of the range from the BAM size (will require a new call in the Storage interface)
    Ok(vec![
      BytesRange::default().with_start(start.bytes_range_start())
    ])
  }

  /// This returns reads for a given reference name and an optional sequence range
  fn get_byte_ranges_for_reference_name(
    &self,
    bam_key: &str,
    reference_name: &str,
    bai_index: &bai::Index,
    query: &Query,
  ) -> Result<Vec<BytesRange>> {
    let get_options = GetOptions::default().with_max_length(Self::DEFAULT_BAM_HEADER_LENGTH);
    let bam_path = self.storage.get(bam_key, get_options)?;
    let (mut bam_reader, bam_header) = Self::read_bam_header(&bam_path)?;
    let maybe_bam_ref_seq = bam_header.reference_sequences().get_full(reference_name);

    let byte_ranges = match maybe_bam_ref_seq {
      None => Err(HtsGetError::not_found(format!(
        "Reference name not found: {}",
        reference_name
      ))),
      Some((bam_ref_seq_idx, _, bam_ref_seq)) => {
        let seq_start = query.start.map(|start| start as i32);
        let seq_end = query.end.map(|end| end as i32);
        Self::get_byte_ranges_for_reference_sequence(
          bam_ref_seq,
          bam_ref_seq_idx,
          bai_index,
          seq_start,
          seq_end,
          &mut bam_reader,
        )
      }
    }?;
    Ok(byte_ranges)
  }

  fn read_bam_header<P: AsRef<Path>>(path: P) -> Result<(bam::Reader<File>, sam::Header)> {
    let mut bam_reader = Self::get_bam_reader(path)?;

    let bam_header = bam_reader
      .read_header()
      .map_err(|_| HtsGetError::io_error("Reading BAM header"))?
      .parse()
      .map_err(|_| HtsGetError::io_error("Parsing BAM header"))?;

    Ok((bam_reader, bam_header))
  }

  fn get_bam_reader<P: AsRef<Path>>(path: P) -> Result<bam::Reader<File>> {
    File::open(path.as_ref())
      .map(bam::Reader::new)
      .map_err(|_| HtsGetError::io_error("Reading BAM"))
  }

  fn get_byte_ranges_for_reference_sequence(
    bam_ref_seq: &sam::header::ReferenceSequence,
    bam_ref_seq_idx: usize,
    bai_index: &bai::Index,
    seq_start: Option<i32>,
    seq_end: Option<i32>,
    bam_reader: &mut bam::Reader<File>,
  ) -> Result<Vec<BytesRange>> {
    let seq_start = seq_start.unwrap_or(Self::MIN_SEQ_POSITION as i32);
    let seq_end = seq_end.unwrap_or_else(|| bam_ref_seq.len());
    let bai_ref_seq = bai_index
      .reference_sequences()
      .get(bam_ref_seq_idx)
      .ok_or_else(|| {
        HtsGetError::not_found(format!(
          "Reference not found in the BAI file: {} ({})",
          bam_ref_seq.name(),
          bam_ref_seq_idx
        ))
      })?;

    let chunks: Vec<Chunk> = bai_ref_seq
      .query(seq_start..=seq_end)
      .map_err(|_| HtsGetError::InvalidRange(format!("{}-{}", seq_start, seq_end)))?
      .into_iter()
      .flat_map(|bin| bin.chunks())
      .cloned()
      .collect();

    let min_offset = bai_ref_seq.min_offset(seq_start);

    let byte_ranges = optimize_chunks(&chunks, min_offset)
      .into_iter()
      .map(|chunk| {
        BytesRange::default()
          .with_start(chunk.start().bytes_range_start())
          .with_end(chunk.end().bytes_range_end(bam_reader))
      })
      .collect();

    Ok(BytesRange::merge_all(byte_ranges))
  }

  /// Returns the header bytes range.
  fn get_byte_ranges_for_header(&self) -> Vec<BytesRange> {
    vec![BytesRange::default()
      .with_start(0)
      .with_end(Self::DEFAULT_BAM_HEADER_LENGTH)]
  }

  /// Build the response from the query using urls.
  fn build_response(
    &self,
    query: Query,
    bam_key: &str,
    byte_ranges: Vec<BytesRange>,
  ) -> Result<Response> {
    let urls = byte_ranges
      .into_iter()
      .map(|range| {
        let options = UrlOptions::default()
          .with_range(range)
          .with_class(query.class.clone());
        self
          .storage
          .url(&bam_key, options)
          .map_err(HtsGetError::from)
      })
      .collect::<Result<Vec<Url>>>()?;

    let format = query.format.unwrap_or(Format::Bam);
    Ok(Response::new(format, urls))
  }
}

#[cfg(test)]
pub mod tests {
  use crate::htsget::Headers;
  use crate::storage::local::LocalStorage;

  use super::*;

  #[test]
  fn search_all_reads() {
    with_local_storage(|storage| {
      let search = BamSearch::new(&storage);
      let query = Query::new("htsnexus_test_NA12878");
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![Url::new(expected_url(&storage))
          .with_headers(Headers::default().with_header("Range", "bytes=4668-"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_unmapped_reads() {
    with_local_storage(|storage| {
      let search = BamSearch::new(&storage);
      let query = Query::new("htsnexus_test_NA12878").with_reference_name("*");
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![Url::new(expected_url(&storage))
          .with_headers(Headers::default().with_header("Range", "bytes=2060795-"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_reference_name_without_seq_range() {
    with_local_storage(|storage| {
      let search = BamSearch::new(&storage);
      let query = Query::new("htsnexus_test_NA12878").with_reference_name("20");
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![Url::new(expected_url(&storage))
          .with_headers(Headers::default().with_header("Range", "bytes=977196-2128166"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_reference_name_with_seq_range() {
    with_local_storage(|storage| {
      let search = BamSearch::new(&storage);
      let query = Query::new("htsnexus_test_NA12878")
        .with_reference_name("11")
        .with_start(5015000)
        .with_end(5050000);
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url(&storage))
            .with_headers(Headers::default().with_header("Range", "bytes=256721-647346")),
          Url::new(expected_url(&storage))
            .with_headers(Headers::default().with_header("Range", "bytes=824361-842101")),
          Url::new(expected_url(&storage))
            .with_headers(Headers::default().with_header("Range", "bytes=977196-996015")),
        ],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_header() {
    with_local_storage(|storage| {
      let search = BamSearch::new(&storage);
      let query = Query::new("htsnexus_test_NA12878").with_class(Class::Header);
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![Url::new(expected_url(&storage))
          .with_headers(Headers::default().with_header("Range", "bytes=0-1048576"))
          .with_class(Class::Header)],
      ));
      assert_eq!(response, expected_response)
    });
  }

  pub fn with_local_storage(test: impl Fn(LocalStorage)) {
    let base_path = std::env::current_dir()
      .unwrap()
      .parent()
      .unwrap()
      .join("data/bam");
    test(LocalStorage::new(base_path).unwrap())
  }

  pub fn expected_url(storage: &LocalStorage) -> String {
    format!(
      "file://{}",
      storage
        .base_path()
        .join("htsnexus_test_NA12878.bam")
        .to_string_lossy()
    )
  }
}
