//! Module providing the search capability using BAM/BAI files
//!

use std::{fs::File};

use noodles_bam::{self as bam, bai};
use noodles_bam::bai::Index;
use noodles_bgzf::index::{Chunk, optimize_chunks};
use noodles_bgzf::VirtualPosition;
use noodles_sam as sam;

use crate::{
  htsget::{Format, HtsGetError, Query, Result},
  htsget::search::{BlockPosition, VirtualPositionExt},
  storage::{BytesRange, GetOptions, Storage},
};
use crate::htsget::search::Search;

pub(crate) struct BamSearch<'a, S> {
  storage: &'a S,
}

impl BlockPosition for bam::Reader<File> {
  fn read_bytes(&mut self) -> Option<usize> {
    self.read_record(&mut bam::Record::default()).ok()
  }

  fn seek(&mut self, pos: VirtualPosition) -> std::io::Result<VirtualPosition> {
    self.seek(pos)
  }

  fn virtual_position(&self) -> VirtualPosition {
    self.virtual_position()
  }
}

impl<'a, S> Search<'a, S, bai::Index> for BamSearch<'a, S>
  where
    S: Storage + 'a
{
  fn get_byte_ranges_for_all(&self, key: &str, index: &Index) -> Result<Vec<BytesRange>> {
    let (mut bam_reader, _) = self.read_bam_header(key)?;
    let mut byte_ranges: Vec<BytesRange> = Vec::new();
    for reference_sequence in index.reference_sequences() {
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
    let unmapped_byte_ranges = self.get_byte_ranges_for_unmapped_reads(key, index)?;
    byte_ranges.extend(unmapped_byte_ranges.into_iter());
    Ok(BytesRange::merge_all(byte_ranges))
  }

  fn get_byte_ranges_for_reference_name(&self, key: &str, reference_name: &str, index: &Index, query: &Query) -> Result<Vec<BytesRange>> {
    if reference_name == "*" {
      self.get_byte_ranges_for_unmapped_reads(key, index)
    } else {
      self.get_byte_ranges_for_reference_name(key, reference_name, index, query)
    }
  }

  fn read_index(&self, key: &str) -> Result<Index> {
    let bai_path = self.storage.get(&key, GetOptions::default())?;
    bai::read(bai_path).map_err(|_| HtsGetError::io_error("Reading BAI"))
  }

  fn get_keys_from_id(&self, id: &str) -> (String, String) {
    let bam_key = format!("{}.bam", id);
    let bai_key = format!("{}.bai", bam_key);
    (bam_key, bai_key)
  }

  fn get_byte_ranges_for_header(&self, key: &str) -> Result<Vec<BytesRange>> {
    let (mut reader, _) = self.read_bam_header(key)?;
    reader.read_reference_sequences()?;
    Ok(vec![BytesRange::default().with_start(0).with_end(
      reader.virtual_position().bytes_range_end(&mut reader),
    )])
  }

  fn get_storage(&self) -> &S {
    self.storage
  }

  fn get_format(&self) -> Format {
    Format::Bam
  }
}

impl<'a, S> BamSearch<'a, S>
where
  S: Storage + 'a,
{
  const MIN_SEQ_POSITION: u32 = 1; // 1-based

  pub fn new(storage: &'a S) -> Self {
    Self { storage }
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
        let (bam_reader, _) = self.read_bam_header(bam_key)?;
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
    let (mut bam_reader, bam_header) = self.read_bam_header(bam_key)?;
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

  fn read_bam_header(&self, key: &str) -> Result<(bam::Reader<File>, sam::Header)> {
    let mut bam_reader = self.get_reader(key, "Reading BAM", bam::Reader::new)?;

    let bam_header = bam_reader
      .read_header()
      .map_err(|_| HtsGetError::io_error("Reading BAM header"))?
      .parse()
      .map_err(|_| HtsGetError::io_error("Parsing BAM header"))?;

    Ok((bam_reader, bam_header))
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
}

#[cfg(test)]
pub mod tests {
  use crate::htsget::{Class, Headers, Response, Url};
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
          .with_headers(Headers::default().with_header("Range", "bytes=0-4668"))
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
