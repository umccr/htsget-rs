use std::convert::TryFrom;
use std::fs::File;
use std::io::BufReader;

use noodles_bam::record::ReferenceSequenceId;
use noodles_cram::{crai, Reader};
use noodles_cram::crai::{Index, Record};
use noodles_sam::Header;

use crate::htsget::{Class, Format, HtsGetError, Query, Response, Result, Url};
use crate::storage::{BytesRange, GetOptions, Storage, UrlOptions};

pub(crate) struct CramSearch<'a, S> {
  storage: &'a S,
}

impl<'a, S> CramSearch<'a, S>
  where
    S: Storage + 'a
{
  const FILE_DEFINITION_LENGTH: u64 = 26;
  const EOF_CONTAINER_LENGTH: u64 = 38;
  const MIN_SEQ_POSITION: u32 = 1; // 1-based

  pub fn new(storage: &'a S) -> Self {
    Self { storage }
  }

  /// Build response from a query.
  pub fn search(&self, query: Query) -> Result<Response> {
    let (cram_key, crai_key) = self.get_keys_from_id(query.id.as_str());
    let crai_index = self.read_index(&crai_key)?;
    let header_bytes = self.get_byte_ranges_for_header(&crai_index)?;

    // Currently this ignores the start and end part of the query.
    match query.class {
      Class::Body => {
        let (mut cram_reader, cram_header) = self.read_header(&cram_key, header_bytes)?;
        let byte_ranges = match query.reference_name.as_ref() {
          None => self.get_byte_ranges_for_all_reads(&crai_index, &mut cram_reader)?,
          Some(reference_name) if reference_name.as_str() == "*" => {
            self.get_byte_ranges_for_unmapped_reads(&crai_index, &mut cram_reader)?
          }
          Some(reference_name) => self.get_byte_ranges_for_reference_name(
              reference_name,
              &crai_index,
              &mut cram_reader,
              cram_header,
              &query,
          )?,
        };
        self.build_response(query, &cram_key, byte_ranges)
      }
      Class::Header => {
        self.build_response(query, &cram_key, vec![header_bytes])
      }
    }
  }

  /// Read index from key
  fn read_index(&self, crai_key: &str) -> Result<Index> {
    let crai_path = self.storage.get(&crai_key, GetOptions::default())?;

    crai::read(crai_path).map_err(|_| HtsGetError::io_error("Reading CRAI"))
  }

  /// Returns the header bytes range.
  fn get_byte_ranges_for_header(
    &self,
    crai_index: &[crai::Record],
  ) -> Result<BytesRange> {
    // Assuming that the first index represents the first data container.
    let first_record = crai_index.first().ok_or_else(|| HtsGetError::not_found("No entries in CRAI"))?;
    Ok(BytesRange::default()
      .with_start(Self::FILE_DEFINITION_LENGTH)
      .with_end(first_record.offset())
    )
  }

  /// Read header using storage options.
  fn read_header(
    &self,
    key: &str,
    header_bytes: BytesRange,
  ) -> Result<(Reader<BufReader<File>>, Header)> {
    let get_options = GetOptions::default().with_range(header_bytes);
    let cram_path = self.storage.get(key, get_options)?;

    let mut reader = File::open(cram_path)
      .map(BufReader::new)
      .map(noodles_cram::Reader::new)
      .map_err(|_| HtsGetError::io_error("Reading CRAM"))?;

    reader.read_file_definition().map_err(|_| HtsGetError::io_error("Reading CRAM file definition"))?;

    let header = reader
      .read_file_header()
      .map_err(|_| HtsGetError::io_error("Reading CRAM header"))?
      .parse()
      .map_err(|_| HtsGetError::io_error("Parsing CRAM header"))?;

    Ok((reader, header))
  }

  /// Get key for storage object.
  fn get_keys_from_id(&self, id: &str) -> (String, String) {
    let cram_key = format!("{}.cram", id);
    let crai_key = format!("{}.crai", cram_key);
    (cram_key, crai_key)
  }

  /// Returns mapped and placed unmapped ranges
  fn get_byte_ranges_for_all_reads(
    &self,
    crai_index: &[crai::Record],
    cram_reader: &mut Reader<BufReader<File>>,
  ) -> Result<Vec<BytesRange>> {
    Self::bytes_ranges_from_index(
      None,
      None,
      None,
      crai_index,
      cram_reader,
      |_| true,
    )
  }

  /// Returns only unplaced unmapped ranges
  fn get_byte_ranges_for_unmapped_reads(
    &self,
    crai_index: &[crai::Record],
    cram_reader: &mut Reader<BufReader<File>>,
  ) -> Result<Vec<BytesRange>> {
    Self::bytes_ranges_from_index(
      None,
      None,
      None,
      crai_index,
      cram_reader,
      |record| {
        record.reference_sequence_id().is_none()
      },
    )
  }

  /// Returns reads for a given reference name and an optional sequence range
  fn get_byte_ranges_for_reference_name(
    &self,
    reference_name: &str,
    crai_index: &[crai::Record],
    cram_reader: &mut Reader<BufReader<File>>,
    cram_header: Header,
    query: &Query,
  ) -> Result<Vec<BytesRange>> {
    let maybe_cram_ref_seq = cram_header.reference_sequences().get_full(reference_name);

    let byte_ranges = match maybe_cram_ref_seq {
      None => Err(HtsGetError::not_found(format!(
        "Reference name not found: {}",
        reference_name
      ))),
      Some((ref_seq_id, _, ref_seq)) => {
        let cram_ref_seq_idx = ReferenceSequenceId::try_from(ref_seq_id as i32)
          .map_err(|_| HtsGetError::invalid_input("Invalid reference sequence id"))?;
        let seq_start = query.start.map(|start| start as i32);
        let seq_end = query.end.map(|end| end as i32);
        Self::bytes_ranges_from_index(
          Some(ref_seq),
          seq_start,
          seq_end,
          crai_index,
          cram_reader,
          |record| record.reference_sequence_id() == Some(cram_ref_seq_idx),
        )
      }
    }?;
    Ok(byte_ranges)
  }

  /// Get bytes ranges using the index.
  fn bytes_ranges_from_index<F>(
    ref_seq: Option<&noodles_sam::header::ReferenceSequence>,
    seq_start: Option<i32>,
    seq_end: Option<i32>,
    crai_index: &[crai::Record],
    cram_reader: &mut noodles_cram::Reader<BufReader<File>>,
    predicate: F,
  ) -> Result<Vec<BytesRange>>
    where F: Fn(&Record) -> bool
  {
    // This could be improved by using some sort of index mapping.
    let mut byte_ranges: Vec<BytesRange> = crai_index.iter().zip(crai_index.iter().skip(1))
      .filter_map(|(record, next)| {
        if predicate(record) {
          Self::bytes_ranges_for_record(ref_seq, seq_start, seq_end, record, next)
        } else {
          None
        }
      }).collect();

    let last = crai_index.last().ok_or_else(|| HtsGetError::invalid_input("No entries in CRAI"))?;
    if predicate(last) {
      // An implementation based on file size might be better.
      cram_reader.seek(std::io::SeekFrom::Start(last.offset()))?;
      cram_reader.records().last();
      let eof_position = cram_reader.position().map_err(|_| HtsGetError::io_error("Reading CRAM eof"))?;
      let eof_position = eof_position - Self::EOF_CONTAINER_LENGTH;
      byte_ranges.push(BytesRange::default().with_start(last.offset()).with_end(eof_position));
    }

    Ok(BytesRange::merge_all(byte_ranges))
  }

  /// Gets bytes ranges for a specific index entry.
  fn bytes_ranges_for_record(
      ref_seq: Option<&noodles_sam::header::ReferenceSequence>,
      seq_start: Option<i32>,
      seq_end: Option<i32>,
      record: &Record,
      next: &Record,
  ) -> Option<BytesRange> {
    match ref_seq {
      None => {
        Some(BytesRange::default().with_start(record.offset()).with_end(next.offset()))
      }
      Some(ref_seq) => {
        let seq_start = seq_start.unwrap_or(Self::MIN_SEQ_POSITION as i32);
        let seq_end = seq_end.unwrap_or_else(|| ref_seq.len());

        if seq_start <= record.alignment_start() + record.alignment_span() && seq_end >= record.alignment_start() {
          Some(BytesRange::default().with_start(record.offset()).with_end(next.offset()))
        } else {
          None
        }
      }
    }
  }

  /// Build the response from the query using urls.
  fn build_response(
    &self,
    query: Query,
    cram_key: &str,
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
          .url(&cram_key, options)
          .map_err(HtsGetError::from)
      })
      .collect::<Result<Vec<Url>>>()?;

    let format = query.format.unwrap_or(Format::Cram);
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
    test(LocalStorage::new(base_path).unwrap())
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
