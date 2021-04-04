//! Module providing the search capability using BAM/BAI files
//!

use std::{fs::File, path::Path};

use noodles_bam::{self as bam, bai};
use noodles_bgzf::VirtualPosition;
use noodles_sam::{self as sam};
use sam::header::ReferenceSequences;

use crate::{
  htsget::{Format, HtsGetError, Query, Response, Result, Url},
  storage::{GetOptions, Range, Storage, UrlOptions},
};

pub(crate) struct BamSearch<'a, S> {
  storage: &'a S,
}

impl<'a, S> BamSearch<'a, S>
where
  S: Storage,
{
  /// 100 Mb
  const DEFAULT_BAM_HEADER_LENGTH: u64 = 100 * 1024 * 1024; // TODO find a number that makes more sense

  pub fn new(storage: &'a S) -> Self {
    Self { storage }
  }

  pub fn search(&self, query: Query) -> Result<Response> {
    // TODO check class, for now we assume it is None or "body"

    let (bam_key, bai_key) = self.get_keys_from_id(query.id.as_str())?;

    let bai_path = self.storage.get(&bai_key, GetOptions::default())?;
    let index = bai::read(bai_path).map_err(|_| HtsGetError::io_error("Reading BAI"))?;

    let positions = match query.reference_name.as_ref() {
      None => Self::get_positions_for_mapped_reads(&index),
      Some(reference_name) if reference_name.as_str() == "*" => {
        Self::get_positions_for_all_reads(&index)
      }
      Some(reference_name) => {
        let get_options = GetOptions::default().with_max_length(Self::DEFAULT_BAM_HEADER_LENGTH);
        let bam_path = self.storage.get(&bam_key, get_options)?;
        let bam_header = Self::read_bam_header(&bam_path)?;
        let reference_sequences = bam_header.reference_sequences();
        match reference_sequences.get(reference_name) {
          None => Err(HtsGetError::not_found(format!(
            "Reference name not found: {}",
            reference_name
          ))),
          Some(reference_sequence) => {
            // let region = self.get_region_from_query(&query, reference_sequences)?;
            // let q = bam_reader.query(reference_sequences, &index, &region)
            //   .map_err(|_| HtsGetError::IOError("Querying BAM".to_string()))?;
            unimplemented!()
          }
        }?
      }
    };

    let urls = positions
      .into_iter()
      .map(|(start, end)| {
        let range = Range::new()
          .with_start(start.compressed())
          .with_end(end.compressed());
        let options = UrlOptions::default().with_range(range);
        self
          .storage
          .url(&bam_key, options)
          .map_err(HtsGetError::from)
      })
      .collect::<Result<Vec<Url>>>()?;

    let format = query.format.unwrap_or(Format::BAM);
    Ok(Response::new(format, urls))
  }

  /// Generate a key for the storage object from an ID
  /// This may involve a more complex transformation in the future,
  /// or even require custom implementations depending on the organizational structure
  /// For now there is a 1:1 mapping to the underlying files
  fn get_keys_from_id(&self, id: &str) -> Result<(String, String)> {
    let bam_key = format!("{}.bam", id);
    let bai_key = format!("{}.bai", bam_key);
    Ok((bam_key, bai_key))
  }

  // This returns only mapped reads
  fn get_positions_for_mapped_reads(index: &bai::Index) -> Vec<(VirtualPosition, VirtualPosition)> {
    let mut positions: Vec<(VirtualPosition, VirtualPosition)> = Vec::new();
    for reference_sequence in index.reference_sequences() {
      if let Some(metadata) = reference_sequence.metadata() {
        // TODO Report to the noodles author that the call to reference_sequence.min_offset(0) will panic, can we avoid that panic?
        let start_vpos = reference_sequence.min_offset(1);
        let end_vpos = metadata.end_position();
        positions.push((start_vpos, end_vpos));
      }
    }
    positions
  }

  // This returns unplaced unmapped and mapped reads
  fn get_positions_for_all_reads(index: &bai::Index) -> Vec<(VirtualPosition, VirtualPosition)> {
    let mut positions: Vec<(VirtualPosition, VirtualPosition)> = Vec::new();
    for reference_sequence in index.reference_sequences() {
      if let Some(metadata) = reference_sequence.metadata() {
        // TODO Ask to the noodles author whether metadata.start_position will include unmapped reads or not
        let start_vpos = metadata.start_position();
        let end_vpos = metadata.end_position();
        positions.push((start_vpos, end_vpos));
      }
    }
    positions
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
}

#[cfg(test)]
pub mod tests {

  use super::*;
  use crate::htsget::Headers;
  use crate::storage::local::LocalStorage;

  use bam_builder::{bam_order::BamSortOrder, BamBuilder};

  #[test]
  fn search_mapped_reads() {
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
        Format::BAM,
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
  fn search_all_reads() {
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
        Format::BAM,
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

  #[test]
  fn search_unmapped_reads() {
        // Create a builder with all defaults except the read_len is 100
        let mut builder = BamBuilder::new(
          100,                        // default read length
          30,                         // default base quality
          "HtsGetTestBamUnmapped".to_owned(), // name of sample
          None,                       // optional read group id
          BamSortOrder::Unsorted,     // how to sort reads when `.sort` is called
          None,                       // optional sequence dictionary
          Some(666),                  // optional seed used for generating random bases
      );

      // Create a single read pair with only 2 unmapped reads
      let records = builder
          .pair_builder()
          .contig(0)               // reads are mapped to tid 0
          .start1(0)               // start pos of read1
          .start2(200)             // start pos of read2
          .unmapped1(true)         // override default of unmapped
          .unmapped2(true)         // override default of unmapped
          .build()                 // inflate the underlying records and set mate info
          .unwrap();

      // Add the pair to bam builder
      builder.add_pair(records);

      // Are we unmapped yet?
      assert_eq!(builder.records[0].tid(), -1);
      assert_eq!(builder.records[0].pos(), -1);

      // Do htsget search on those unmapped reads
      with_local_storage(|storage| {

        let search = BamSearch::new(&storage);
        let query = Query::new("HtsGetTestBamUnmapped").with_reference_name("*");
        let response = search.search(query);
        println!("{:#?}", response);
        let expected_url = format!(
          "data://{}", // TODO: Storage backend should be just bytes instead of files in filesystem?
                       // Also, assumes there's a HtsGetTestBamUnmapped.bam.bai present... perhaps I should just generate files and dump to disk first as a PoC?
          storage
            .base_path()
            .join("inline_data_perhaps")
            .to_string_lossy()
        );
        let expected_response = Ok(Response::new(
          Format::BAM,
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

  // TODO add tests for `BamSearch::url`
  
  pub fn with_local_storage(test: impl Fn(LocalStorage)) {
    let base_path = std::env::current_dir()
      .unwrap()
      .parent()
      .unwrap()
      .join("data");
    test(LocalStorage::new(base_path).unwrap())
  }
}
