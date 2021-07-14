//! Module providing the search capability using BAM/BAI files
//!

use std::path::PathBuf;
use std::{fs::File, io};

use noodles_bam::bai::index::ReferenceSequence;
use noodles_bam::bai::Index;
use noodles_bam::{self as bam, bai, Reader};
use noodles_bgzf::VirtualPosition;
use noodles_csi::BinningIndex;
use noodles_sam as sam;
use noodles_sam::Header;

use crate::htsget::search::{BgzfSearch, Search, SearchReads};
use crate::{
  htsget::search::{BlockPosition, VirtualPositionExt},
  htsget::{Format, Query, Result},
  storage::{BytesRange, Storage},
};

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

impl<'a, S> BgzfSearch<'a, S, ReferenceSequence, bai::Index, bam::Reader<File>, sam::Header>
  for BamSearch<'a, S>
where
  S: Storage + 'a,
{
  type ReferenceSequenceHeader = sam::header::ReferenceSequence;

  fn max_seq_position(ref_seq: &Self::ReferenceSequenceHeader) -> i32 {
    ref_seq.len()
  }

  fn get_byte_ranges_for_unmapped(
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
        let (bam_reader, _) = self.create_reader(bam_key)?;
        bam_reader.virtual_position()
      }
    };

    // TODO get the end of the range from the BAM size (will require a new call in the Storage interface)
    Ok(vec![
      BytesRange::default().with_start(start.bytes_range_start())
    ])
  }
}

impl<'a, S> Search<'a, S, ReferenceSequence, bai::Index, bam::Reader<File>, sam::Header>
  for BamSearch<'a, S>
where
  S: Storage + 'a,
{
  const READER_FN: fn(File) -> Reader<File> = bam::Reader::new;
  const HEADER_FN: fn(&mut Reader<File>) -> io::Result<String> = |reader| {
    let header = reader.read_header();
    reader.read_reference_sequences()?;
    header
  };
  const INDEX_FN: fn(PathBuf) -> io::Result<Index> = bai::read;

  fn get_byte_ranges_for_reference_name(
    &self,
    key: &str,
    reference_name: &str,
    index: &Index,
    query: &Query,
  ) -> Result<Vec<BytesRange>> {
    self.get_byte_ranges_for_reference_name_reads(key, reference_name, index, query)
  }

  fn get_keys_from_id(&self, id: &str) -> (String, String) {
    let bam_key = format!("{}.bam", id);
    let bai_key = format!("{}.bai", bam_key);
    (bam_key, bai_key)
  }

  fn get_storage(&self) -> &S {
    self.storage
  }

  fn get_format(&self) -> Format {
    Format::Bam
  }
}

impl<'a, S> SearchReads<'a, S, ReferenceSequence, bai::Index, bam::Reader<File>, sam::Header>
  for BamSearch<'a, S>
where
  S: Storage + 'a,
{
  fn get_reference_sequence_from_name<'b>(
    &self,
    header: &'b Header,
    name: &str,
  ) -> Option<(
    usize,
    &'b String,
    &'b noodles_sam::header::ReferenceSequence,
  )> {
    header.reference_sequences().get_full(name)
  }

  fn get_byte_ranges_for_unmapped_reads(
    &self,
    bam_key: &str,
    bai_index: &Index,
  ) -> Result<Vec<BytesRange>> {
    self.get_byte_ranges_for_unmapped(bam_key, bai_index)
  }

  fn get_byte_ranges_for_reference_sequence(
    &self,
    ref_seq: &sam::header::ReferenceSequence,
    ref_seq_id: usize,
    query: &Query,
    index: &Index,
    reader: &mut Reader<File>,
  ) -> Result<Vec<BytesRange>> {
    self.get_byte_ranges_for_reference_sequence_bgzf(
      ref_seq,
      ref_seq_id,
      &index,
      query.start.map(|start| start as i32),
      query.end.map(|end| end as i32),
      reader,
    )
  }
}

impl<'a, S> BamSearch<'a, S>
where
  S: Storage + 'a,
{
  pub fn new(storage: &'a S) -> Self {
    Self { storage }
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
