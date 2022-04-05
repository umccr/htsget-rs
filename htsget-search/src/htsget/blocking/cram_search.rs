//! This module provides search capabilities for CRAM files.
//!

use std::fs::File;
use std::io;
use std::marker::PhantomData;
use std::path::PathBuf;

use noodles::cram;
use noodles::cram::crai::{Index, Record};
use noodles::cram::{crai, Reader};
use noodles::sam;
use noodles::sam::Header;

use crate::htsget::blocking::search::{Search, SearchAll, SearchReads};
use crate::htsget::{Format, HtsGetError, Query, Result};
use crate::storage::blocking::Storage;
use crate::storage::BytesRange;

pub(crate) struct CramSearch<'a, S> {
  storage: &'a S,
}

impl<'a, S> SearchAll<'a, S, PhantomData<Self>, Index, Reader<File>, Header> for CramSearch<'a, S>
where
  S: Storage + 'a,
{
  fn get_byte_ranges_for_all(&self, key: &str, index: &Index) -> Result<Vec<BytesRange>> {
    Self::bytes_ranges_from_index(self, key, None, None, None, index, |_| true)
  }

  fn get_byte_ranges_for_header(&self, key: &str) -> Result<Vec<BytesRange>> {
    let (mut reader, _) = self.create_reader(key)?;
    Ok(vec![BytesRange::default()
      .with_start(Self::FILE_DEFINITION_LENGTH)
      .with_end(reader.position()?)])
  }
}

impl<'a, S> SearchReads<'a, S, PhantomData<Self>, Index, Reader<File>, Header> for CramSearch<'a, S>
where
  S: Storage + 'a,
{
  fn get_reference_sequence_from_name<'b>(
    &self,
    header: &'b Header,
    name: &str,
  ) -> Option<(usize, &'b String, &'b sam::header::ReferenceSequence)> {
    header.reference_sequences().get_full(name)
  }

  fn get_byte_ranges_for_unmapped_reads(
    &self,
    key: &str,
    index: &Index,
  ) -> Result<Vec<BytesRange>> {
    Self::bytes_ranges_from_index(self, key, None, None, None, index, |record| {
      record.reference_sequence_id().is_none()
    })
  }

  fn get_byte_ranges_for_reference_sequence(
    &self,
    key: &str,
    ref_seq: &sam::header::ReferenceSequence,
    ref_seq_id: usize,
    query: &Query,
    index: &Index,
    _reader: &mut Reader<File>,
  ) -> Result<Vec<BytesRange>> {
    Self::bytes_ranges_from_index(
      self,
      key,
      Some(ref_seq),
      query.start.map(|start| start as i32),
      query.end.map(|end| end as i32),
      index,
      |record| record.reference_sequence_id() == Some(ref_seq_id),
    )
  }
}

impl<'a, S> Search<'a, S, PhantomData<Self>, Index, Reader<File>, Header> for CramSearch<'a, S>
where
  S: Storage + 'a,
{
  const READER_FN: fn(File) -> Reader<File> = cram::Reader::new;
  const HEADER_FN: fn(&mut Reader<File>) -> io::Result<String> = |reader| {
    reader.read_file_definition()?;
    reader.read_file_header()
  };
  const INDEX_FN: fn(PathBuf) -> io::Result<Index> = crai::read;

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
    let cram_key = format!("{}.cram", id);
    let crai_key = format!("{}.crai", cram_key);
    (cram_key, crai_key)
  }

  fn get_storage(&self) -> &S {
    self.storage
  }

  fn get_format(&self) -> Format {
    Format::Cram
  }
}

impl<'a, S> CramSearch<'a, S>
where
  S: Storage + 'a,
{
  const FILE_DEFINITION_LENGTH: u64 = 26;
  const EOF_CONTAINER_LENGTH: u64 = 38;

  pub fn new(storage: &'a S) -> Self {
    Self { storage }
  }

  /// Get bytes ranges using the index.
  fn bytes_ranges_from_index<F>(
    &self,
    key: &str,
    ref_seq: Option<&sam::header::ReferenceSequence>,
    seq_start: Option<i32>,
    seq_end: Option<i32>,
    crai_index: &[crai::Record],
    predicate: F,
  ) -> Result<Vec<BytesRange>>
  where
    F: Fn(&Record) -> bool,
  {
    // This could be improved by using some sort of index mapping.
    let mut byte_ranges: Vec<BytesRange> = crai_index
      .iter()
      .zip(crai_index.iter().skip(1))
      .filter_map(|(record, next)| {
        if predicate(record) {
          Self::bytes_ranges_for_record(ref_seq, seq_start, seq_end, record, next)
        } else {
          None
        }
      })
      .collect();

    let last = crai_index
      .last()
      .ok_or_else(|| HtsGetError::invalid_input("No entries in CRAI"))?;
    if predicate(last) {
      let file_size = self
        .storage
        .head(key)
        .map_err(|_| HtsGetError::io_error("Reading CRAM file size."))?;
      let eof_position = file_size - Self::EOF_CONTAINER_LENGTH;
      byte_ranges.push(
        BytesRange::default()
          .with_start(last.offset())
          .with_end(eof_position),
      );
    }

    Ok(BytesRange::merge_all(byte_ranges))
  }

  /// Gets bytes ranges for a specific index entry.
  pub(crate) fn bytes_ranges_for_record(
    ref_seq: Option<&sam::header::ReferenceSequence>,
    seq_start: Option<i32>,
    seq_end: Option<i32>,
    record: &Record,
    next: &Record,
  ) -> Option<BytesRange> {
    match ref_seq {
      None => Some(
        BytesRange::default()
          .with_start(record.offset())
          .with_end(next.offset()),
      ),
      Some(ref_seq) => {
        let seq_start = seq_start.unwrap_or(Self::MIN_SEQ_POSITION as i32);
        let seq_end = seq_end.unwrap_or_else(|| ref_seq.len());

        let start = record
          .alignment_start()
          .map(usize::from)
          .unwrap_or_default() as i32;
        if seq_start <= start + record.alignment_span() as i32 && seq_end >= start {
          Some(
            BytesRange::default()
              .with_start(record.offset())
              .with_end(next.offset()),
          )
        } else {
          None
        }
      }
    }
  }
}

#[cfg(test)]
pub mod tests {
  use crate::htsget::{Class, Headers, Response, Url};
  use crate::storage::blocking::local::LocalStorage;
  use htsget_config::regex_resolver::RegexResolver;

  use super::*;

  #[test]
  fn search_all_reads() {
    with_local_storage(|storage| {
      let search = CramSearch::new(&storage);
      let query = Query::new("htsnexus_test_NA12878");
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![Url::new(expected_url(&storage))
          .with_headers(Headers::default().with_header("Range", "bytes=6087-1627756"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_unmapped_reads() {
    with_local_storage(|storage| {
      let search = CramSearch::new(&storage);
      let query = Query::new("htsnexus_test_NA12878").with_reference_name("*");
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![Url::new(expected_url(&storage))
          .with_headers(Headers::default().with_header("Range", "bytes=1280106-1627756"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_reference_name_without_seq_range() {
    with_local_storage(|storage| {
      let search = CramSearch::new(&storage);
      let query = Query::new("htsnexus_test_NA12878").with_reference_name("20");
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![Url::new(expected_url(&storage))
          .with_headers(Headers::default().with_header("Range", "bytes=604231-1280106"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_reference_name_with_seq_range_no_overlap() {
    with_local_storage(|storage| {
      let search = CramSearch::new(&storage);
      let query = Query::new("htsnexus_test_NA12878")
        .with_reference_name("11")
        .with_start(5000000)
        .with_end(5050000);
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![Url::new(expected_url(&storage))
          .with_headers(Headers::default().with_header("Range", "bytes=6087-465709"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_reference_name_with_seq_range_overlap() {
    with_local_storage(|storage| {
      let search = CramSearch::new(&storage);
      let query = Query::new("htsnexus_test_NA12878")
        .with_reference_name("11")
        .with_start(5000000)
        .with_end(5100000);
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![Url::new(expected_url(&storage))
          .with_headers(Headers::default().with_header("Range", "bytes=6087-604231"))],
      ));
      assert_eq!(response, expected_response)
    });
  }

  #[test]
  fn search_header() {
    with_local_storage(|storage| {
      let search = CramSearch::new(&storage);
      let query = Query::new("htsnexus_test_NA12878").with_class(Class::Header);
      let response = search.search(query);
      println!("{:#?}", response);

      let expected_response = Ok(Response::new(
        Format::Cram,
        vec![Url::new(expected_url(&storage))
          .with_headers(Headers::default().with_header("Range", "bytes=26-6087"))
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
      .join("data/cram");
    test(LocalStorage::new(base_path, RegexResolver::new(".*", "$0").unwrap()).unwrap())
  }

  pub fn expected_url(storage: &LocalStorage) -> String {
    format!(
      "file://{}",
      storage
        .base_path()
        .join("htsnexus_test_NA12878.cram")
        .to_string_lossy()
    )
  }
}
