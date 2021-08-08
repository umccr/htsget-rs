//! Module providing the search capability using VCF files
//!

use std::marker::PhantomData;
use std::path::PathBuf;
use std::{fs::File, io};

use noodles::bgzf;
use noodles::bgzf::VirtualPosition;
use noodles::tabix;
use noodles::tabix::index::ReferenceSequence;
use noodles::tabix::Index;
use noodles::vcf;
use noodles::vcf::Reader;

use crate::htsget::search::{BgzfSearch, BlockPosition, Search};
use crate::{
  htsget::{Format, HtsGetError, Query, Result},
  storage::{BytesRange, Storage},
};

pub(crate) struct VcfSearch<'a, S> {
  storage: &'a S,
}

impl BlockPosition for vcf::Reader<bgzf::Reader<File>> {
  fn read_bytes(&mut self) -> Option<usize> {
    self.read_record(&mut String::new()).ok()
  }

  fn seek(&mut self, pos: VirtualPosition) -> std::io::Result<VirtualPosition> {
    self.seek(pos)
  }

  fn virtual_position(&self) -> VirtualPosition {
    self.virtual_position()
  }
}

impl<'a, S>
  BgzfSearch<'a, S, ReferenceSequence, tabix::Index, vcf::Reader<bgzf::Reader<File>>, vcf::Header>
  for VcfSearch<'a, S>
where
  S: Storage + 'a,
{
  type ReferenceSequenceHeader = PhantomData<Self>;

  fn max_seq_position(_ref_seq: &Self::ReferenceSequenceHeader) -> i32 {
    Self::MAX_SEQ_POSITION
  }
}

impl<'a, S>
  Search<'a, S, ReferenceSequence, tabix::Index, vcf::Reader<bgzf::Reader<File>>, vcf::Header>
  for VcfSearch<'a, S>
where
  S: Storage + 'a,
{
  const READER_FN: fn(File) -> Reader<bgzf::Reader<File>> =
    |file| vcf::Reader::new(bgzf::Reader::new(file));
  const HEADER_FN: fn(&mut Reader<bgzf::Reader<File>>) -> io::Result<String> =
    vcf::Reader::read_header;
  const INDEX_FN: fn(PathBuf) -> io::Result<Index> = tabix::read;

  fn get_byte_ranges_for_reference_name(
    &self,
    key: &str,
    reference_name: &str,
    index: &Index,
    query: &Query,
  ) -> Result<Vec<BytesRange>> {
    let (mut vcf_reader, vcf_header) = self.create_reader(key)?;
    let maybe_len = vcf_header
      .contigs()
      .get(reference_name)
      .and_then(|contig| contig.len());

    // We are assuming the order of the names and the references sequences
    // in the index is the same
    let ref_seq_index = index
      .reference_sequence_names()
      .iter()
      .enumerate()
      .find(|(_, name)| name == &reference_name)
      .map(|(index, _)| index)
      .ok_or_else(|| {
        HtsGetError::not_found(format!(
          "Reference name not found in the TBI file: {}",
          reference_name,
        ))
      })?;

    let seq_start = query.start.map(|start| start as i32);
    let seq_end = query.end.map(|end| end as i32).or(maybe_len);
    let byte_ranges = self.get_byte_ranges_for_reference_sequence_bgzf(
      &PhantomData,
      ref_seq_index,
      index,
      seq_start,
      seq_end,
      &mut vcf_reader,
    )?;
    Ok(byte_ranges)
  }

  fn get_keys_from_id(&self, id: &str) -> (String, String) {
    let vcf_key = format!("{}.vcf.gz", id);
    let tbi_key = format!("{}.vcf.gz.tbi", id);
    (vcf_key, tbi_key)
  }

  fn get_storage(&self) -> &S {
    self.storage
  }

  fn get_format(&self) -> Format {
    Format::Vcf
  }
}

impl<'a, S> VcfSearch<'a, S>
where
  S: Storage + 'a,
{
  // 1-based
  const MAX_SEQ_POSITION: i32 = (1 << 29) - 1; // see https://github.com/zaeleus/noodles/issues/25#issuecomment-868871298

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
  fn search_all_variants() {
    with_local_storage(|storage| {
      let search = VcfSearch::new(&storage);
      let filename = "sample1-bcbio-cancer";
      let query = Query::new(filename);
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(expected_url(filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-3367"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_reference_name_without_seq_range() {
    with_local_storage(|storage| {
      let search = VcfSearch::new(&storage);
      let filename = "spec-v4.3";
      let query = Query::new(filename).with_reference_name("20");
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(expected_url(filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-823"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_reference_name_with_seq_range() {
    with_local_storage(|storage| {
      let search = VcfSearch::new(&storage);
      let filename = "sample1-bcbio-cancer";
      let query = Query::new(filename)
        .with_reference_name("chrM")
        .with_start(151)
        .with_end(153);
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(expected_url(filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-3367"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_reference_name_with_invalid_seq_range() {
    with_local_storage(|storage| {
      let search = VcfSearch::new(&storage);
      let filename = "sample1-bcbio-cancer";
      let query = Query::new(filename)
        .with_reference_name("chrM")
        .with_start(0)
        .with_end(153);
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Err(HtsGetError::InvalidRange("0-153".to_string()));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_header() {
    with_local_storage(|storage| {
      let search = VcfSearch::new(&storage);
      let filename = "spec-v4.3";
      let query = Query::new(filename).with_class(Class::Header);
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Vcf,
        vec![Url::new(expected_url(filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-823"))
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
      .join("data/vcf");
    test(LocalStorage::new(base_path, "localhost/data").unwrap())
  }

  pub fn expected_url(name: &str) -> String {
    format!("http://localhost/data/{}.vcf.gz", name)
  }
}
