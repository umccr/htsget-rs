//! Module providing the search capability using BCF files
//!

use std::marker::PhantomData;
use std::path::PathBuf;
use std::{fs::File, io};

use noodles_bcf as bcf;
use noodles_bcf::Reader;
use noodles_bgzf::VirtualPosition;
use noodles_csi::index::ReferenceSequence;
use noodles_csi::{self as csi, Index};
use noodles_vcf as vcf;

use crate::htsget::search::{BgzfSearch, BlockPosition, Search};
use crate::{
  htsget::{Format, HtsGetError, Query, Result},
  storage::{BytesRange, Storage},
};

pub(crate) struct BcfSearch<'a, S> {
  storage: &'a S,
}

impl BlockPosition for bcf::Reader<File> {
  fn read_bytes(&mut self) -> Option<usize> {
    self.read_record(&mut bcf::Record::default()).ok()
  }

  fn seek(&mut self, pos: VirtualPosition) -> std::io::Result<VirtualPosition> {
    self.seek(pos)
  }

  fn virtual_position(&self) -> VirtualPosition {
    self.virtual_position()
  }
}

impl<'a, S> BgzfSearch<'a, S, ReferenceSequence, csi::Index, bcf::Reader<File>, vcf::Header>
  for BcfSearch<'a, S>
where
  S: Storage + 'a,
{
  type ReferenceSequenceHeader = PhantomData<Self>;

  fn max_seq_position(_ref_seq: &Self::ReferenceSequenceHeader) -> i32 {
    Self::MAX_SEQ_POSITION
  }
}

impl<'a, S> Search<'a, S, ReferenceSequence, csi::Index, bcf::Reader<File>, vcf::Header>
  for BcfSearch<'a, S>
where
  S: Storage + 'a,
{
  const READER_FN: fn(File) -> Reader<File> = bcf::Reader::new;
  const HEADER_FN: fn(&mut Reader<File>) -> io::Result<String> = |reader| {
    reader.read_file_format()?;
    reader.read_header()
  };
  const INDEX_FN: fn(PathBuf) -> io::Result<Index> = csi::read;

  fn get_byte_ranges_for_reference_name(
    &self,
    key: &str,
    reference_name: &str,
    index: &Index,
    query: &Query,
  ) -> Result<Vec<BytesRange>> {
    let (mut bcf_reader, header) = self.create_reader(key)?;
    // We are assuming the order of the contigs in the header and the references sequences
    // in the index is the same
    let (ref_seq_index, (_, contig)) = header
      .contigs()
      .iter()
      .enumerate()
      .find(|(_, (name, _))| name == &reference_name)
      .ok_or_else(|| {
        HtsGetError::not_found(format!(
          "Reference name not found in the header: {}",
          reference_name,
        ))
      })?;
    let maybe_len = contig.len();

    let seq_start = query.start.map(|start| start as i32);
    let seq_end = query.end.map(|end| end as i32).or(maybe_len);
    let byte_ranges = self.get_byte_ranges_for_reference_sequence_bgzf(
      &PhantomData,
      ref_seq_index,
      index,
      seq_start,
      seq_end,
      &mut bcf_reader,
    )?;
    Ok(byte_ranges)
  }

  fn get_keys_from_id(&self, id: &str) -> (String, String) {
    let bcf_key = format!("{}.bcf", id);
    let csi_key = format!("{}.bcf.csi", id);
    (bcf_key, csi_key)
  }

  fn get_storage(&self) -> &S {
    self.storage
  }

  fn get_format(&self) -> Format {
    Format::Bcf
  }
}

impl<'a, S> BcfSearch<'a, S>
where
  S: Storage + 'a,
{
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
      let search = BcfSearch::new(&storage);
      let filename = "sample1-bcbio-cancer";
      let query = Query::new(filename);
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bcf,
        vec![Url::new(expected_url(&storage, filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-3530"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_reference_name_without_seq_range() {
    with_local_storage(|storage| {
      let search = BcfSearch::new(&storage);
      let filename = "vcf-spec-v4.3";
      let query = Query::new(filename).with_reference_name("20");
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bcf,
        vec![Url::new(expected_url(&storage, filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-950"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_reference_name_with_seq_range() {
    with_local_storage(|storage| {
      let search = BcfSearch::new(&storage);
      let filename = "sample1-bcbio-cancer";
      let query = Query::new(filename)
        .with_reference_name("chrM")
        .with_start(151)
        .with_end(153);
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bcf,
        vec![Url::new(expected_url(&storage, filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-3530"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_reference_name_with_invalid_seq_range() {
    with_local_storage(|storage| {
      let search = BcfSearch::new(&storage);
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
      let search = BcfSearch::new(&storage);
      let filename = "vcf-spec-v4.3";
      let query = Query::new(filename).with_class(Class::Header);
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Bcf,
        vec![Url::new(expected_url(&storage, filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-950"))
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
      .join("data/bcf");
    test(LocalStorage::new(base_path).unwrap())
  }

  pub fn expected_url(storage: &LocalStorage, name: &str) -> String {
    format!(
      "file://{}",
      storage
        .base_path()
        .join(format!("{}.bcf", name))
        .to_string_lossy()
    )
  }
}
