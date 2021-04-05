//! Module providing the search capability using BAM/BAI files
//!

use std::{fs::File, path::Path};

use bam::bai::index::reference_sequence::bin::Chunk;
use noodles_bam::{self as bam, bai};
use noodles_sam::{self as sam};

use crate::{
  htsget::{Format, HtsGetError, Query, Response, Result, Url},
  storage::{GetOptions, Range, Storage, UrlOptions},
};

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
    // TODO check class, for now we assume it is None or "body"

    let (bam_key, bai_key) = self.get_keys_from_id(query.id.as_str());

    let bai_path = self.storage.get(&bai_key, GetOptions::default())?;
    let bai_index = bai::read(bai_path).map_err(|_| HtsGetError::io_error("Reading BAI"))?;

    let positions = match query.reference_name.as_ref() {
      None => Self::get_positions_for_all_reads(&bai_index),
      Some(reference_name) if reference_name.as_str() == "*" => {
        self.get_positions_for_unmapped_reads(bam_key.as_str(), &bai_index)?
      }
      Some(reference_name) => {
        self.get_positions_for_reference_name(bam_key.as_str(), reference_name, &bai_index, &query)?
      }
    };

    let urls = positions
      .into_iter()
      .map(|range| {
        let options = UrlOptions::default().with_range(range);
        self
          .storage
          .url(&bam_key, options)
          .map_err(HtsGetError::from)
      })
      .collect::<Result<Vec<Url>>>()?;

    let format = query.format.unwrap_or(Format::Bam);
    Ok(Response::new(format, urls))
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

  /// This returns unplaced unmapped and mapped reads
  fn get_positions_for_all_reads(index: &bai::Index) -> Vec<Range> {
    let mut positions: Vec<Range> = Vec::new();
    for reference_sequence in index.reference_sequences() {
      if let Some(metadata) = reference_sequence.metadata() {
        // TODO Ask to the noodles author whether metadata.start_position will include unmapped reads or not
        let start_vpos = metadata.start_position();
        let end_vpos = metadata.end_position();
        positions.push(
          Range::default()
            .with_start(start_vpos.compressed())
            .with_end(end_vpos.compressed()),
        );
      }
    }
    positions
  }

  /// This returns only unmapped reads
  fn get_positions_for_unmapped_reads(
    &self,
    bam_key: &str,
    bai_index: &bai::Index,
  ) -> Result<Vec<Range>> {
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

        let mut bam_reader = File::open(bam_path)
          .map(bam::Reader::new)
          .map_err(|_| HtsGetError::io_error("Reading BAM"))?;

        bam_reader
          .read_header()
          .map_err(|_| HtsGetError::io_error("Reading BAM"))?
          .parse::<sam::Header>()
          .map_err(|_| HtsGetError::io_error("Reading BAM"))?;

        bam_reader.virtual_position()
      }
    };

    Ok(vec![Range::default().with_start(start.compressed())])
  }

  /// This returns reads for a given reference name
  fn get_positions_for_reference_name(
    &self,
    bam_key: &str,
    reference_name: &str,
    bai_index: &bai::Index,
    query: &Query,
  ) -> Result<Vec<Range>> {
    let get_options = GetOptions::default().with_max_length(Self::DEFAULT_BAM_HEADER_LENGTH);
    let bam_path = self.storage.get(bam_key, get_options)?;
    let bam_header = Self::read_bam_header(&bam_path)?;
    let maybe_bam_ref_seq = bam_header.reference_sequences().get_full(reference_name);

    let positions = match maybe_bam_ref_seq {
      None => Err(HtsGetError::not_found(format!(
        "Reference name not found: {}",
        reference_name
      ))),
      Some((bam_ref_seq_idx, _, bam_ref_seq)) => {
        let seq_start = query.start.map(|start| start as i32);
        let seq_end = query.end.map(|end| end as i32);
        Self::get_positions_for_reference_sequence(
          bam_ref_seq,
          bam_ref_seq_idx,
          bai_index,
          seq_start,
          seq_end,
        )
      }
    }?;
    Ok(positions)
  }

  fn read_bam_header<P: AsRef<Path>>(path: P) -> Result<sam::Header> {
    let mut bam_reader = File::open(path.as_ref())
      .map(bam::Reader::new)
      .map_err(|_| HtsGetError::io_error("Reading BAM"))?;

    bam_reader
      .read_header()
      .map_err(|_| HtsGetError::io_error("Reading BAM"))?
      .parse()
      .map_err(|_| HtsGetError::io_error("Reading BAM"))
  }

  fn get_positions_for_reference_sequence(
    bam_ref_seq: &sam::header::ReferenceSequence,
    bam_ref_seq_idx: usize,
    bai_index: &bai::Index,
    seq_start: Option<i32>,
    seq_end: Option<i32>,
  ) -> Result<Vec<Range>> {
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
      .query(seq_start, seq_end)
      .into_iter()
      .flat_map(|bin| bin.chunks())
      .cloned()
      .collect();

    let min_offset = bai_ref_seq.min_offset(seq_start);
    let positions = bai::optimize_chunks(&chunks, min_offset)
      .into_iter()
      .map(|chunk| {
        Range::default()
          .with_start(chunk.start().compressed())
          .with_end(chunk.end().compressed())
      })
      .collect();
    Ok(positions)
  }
}

#[cfg(test)]
pub mod tests {

  use super::*;
  use crate::htsget::Headers;
  use crate::storage::local::LocalStorage;

  #[test]
  fn search_all_reads() {
    with_local_storage(|storage| {
      let search = BamSearch::new(&storage);
      let query = Query::new("htsnexus_test_NA12878");
      let response = search.search(query);
      println!("{:#?}", response);
      let expected_url = format!(
        "file://{}",
        storage
          .base_path()
          .join("htsnexus_test_NA12878.bam")
          .to_string_lossy()
      );
      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![
          Url::new(expected_url.clone())
            .with_headers(Headers::default().with_header("Range", "bytes=4668-977196")),
          Url::new(expected_url)
            .with_headers(Headers::default().with_header("Range", "bytes=977196-2112141")),
        ],
      ));
      assert_eq!(response, expected_response)
    });
  }

  // TODO we need a testing BAM containing unmapped reads
  #[test]
  fn search_unmapped_reads() {
    with_local_storage(|storage| {
      let search = BamSearch::new(&storage);
      let query = Query::new("htsnexus_test_NA12878").with_reference_name("*");
      let response = search.search(query);
      println!("{:#?}", response);
      let expected_url = format!(
        "file://{}",
        storage
          .base_path()
          .join("htsnexus_test_NA12878.bam")
          .to_string_lossy()
      );
      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![Url::new(expected_url)
          .with_headers(Headers::default().with_header("Range", "bytes=2060795-"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_ref_name_without_range() {
    with_local_storage(|storage| {
      let search = BamSearch::new(&storage);
      let query = Query::new("htsnexus_test_NA12878").with_reference_name("20");
      let response = search.search(query);
      println!("{:#?}", response);
      let expected_url = format!(
        "file://{}",
        storage
          .base_path()
          .join("htsnexus_test_NA12878.bam")
          .to_string_lossy()
      );
      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![Url::new(expected_url.clone())
          .with_headers(Headers::default().with_header("Range", "bytes=977196-2112141"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_ref_name_with_range() {
    with_local_storage(|storage| {
      let search = BamSearch::new(&storage);
      let query = Query::new("htsnexus_test_NA12878")
        .with_reference_name("11")
        .with_start(5079500)
        .with_end(5081200);
      let response = search.search(query);
      println!("{:#?}", response);
      let expected_url = format!(
        "file://{}",
        storage
          .base_path()
          .join("htsnexus_test_NA12878.bam")
          .to_string_lossy()
      );
      let expected_response = Ok(Response::new(
        Format::Bam,
        vec![Url::new(expected_url.clone())
          .with_headers(Headers::default().with_header("Range", "bytes=824361-977196"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  pub fn with_local_storage(test: impl Fn(LocalStorage)) {
    let base_path = std::env::current_dir()
      .unwrap()
      .parent()
      .unwrap()
      .join("data");
    test(LocalStorage::new(base_path).unwrap())
  }
}
