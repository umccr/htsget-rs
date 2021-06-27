//! Module providing the search capability using VCF files
//!

use std::{fs::File, path::Path};

use noodles_bgzf::{
  self as bgzf,
  index::{optimize_chunks, Chunk},
  VirtualPosition,
};
use noodles_tabix::{self as tabix};
use noodles_vcf::{self as vcf};

use crate::{
  htsget::{Class, Format, HtsGetError, Query, Response, Result, Url},
  storage::{BytesRange, GetOptions, Storage, UrlOptions},
};

// TODO: This trait is clearly common across, **at least**, VCF and BAM
trait VirtualPositionExt {
  const MAX_BLOCK_SIZE: u64 = 65536;

  /// Get the starting bytes for a compressed BGZF block.
  fn bytes_range_start(&self) -> u64;
  /// Get the ending bytes for a compressed BGZF block.
  fn bytes_range_end(&self, reader: &mut vcf::Reader<bgzf::Reader<File>>) -> u64;
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
  fn bytes_range_end(&self, reader: &mut vcf::Reader<bgzf::Reader<File>>) -> u64 {
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
  reader: &mut vcf::Reader<bgzf::Reader<File>>,
) -> Option<u64> {
  reader.seek(block_position).ok()?.compressed();
  let next_block_index = loop {
    let bytes_read = reader.read_record(&mut String::new()).ok()?;
    let actual_block_index = reader.virtual_position().compressed();
    if bytes_read == 0 || actual_block_index > block_position.compressed() {
      break actual_block_index;
    }
  };
  Some(next_block_index)
}

pub(crate) struct VcfSearch<'a, S> {
  storage: &'a S,
}

impl<'a, S> VcfSearch<'a, S>
where
  S: Storage + 'a,
{
  const MIN_SEQ_POSITION: i32 = 1; // 1-based
  const MAX_SEQ_POSITION: i32 = 37450; // see https://github.com/zaeleus/noodles/issues/25#issuecomment-868871298

  pub fn new(storage: &'a S) -> Self {
    Self { storage }
  }

  pub fn search(&self, query: Query) -> Result<Response> {
    let (vcf_key, tbi_key) = self.get_keys_from_id(query.id.as_str());

    match query.class {
      Class::Body => {
        let tbi_path = self.storage.get(&tbi_key, GetOptions::default())?;
        let vcf_index = tabix::read(tbi_path).map_err(|_| HtsGetError::io_error("Reading TBI"))?;

        let byte_ranges = match query.reference_name.as_ref() {
          None => self.get_byte_ranges_for_all_variants(&vcf_index, &vcf_key)?,
          Some(reference_name) => self.get_byte_ranges_for_reference_name(
            vcf_key.as_str(),
            reference_name,
            &vcf_index,
            &query,
          )?,
        };
        self.build_response(query, &vcf_key, byte_ranges)
      }
      Class::Header => {
        let byte_ranges = self.get_byte_ranges_for_header(vcf_key.as_str())?;
        self.build_response(query, &vcf_key, byte_ranges)
      }
    }
  }

  /// Generate a key for the storage object from an ID
  /// This may involve a more complex transformation in the future,
  /// or even require custom implementations depending on the organizational structure
  /// For now there is a 1:1 mapping to the underlying files
  fn get_keys_from_id(&self, id: &str) -> (String, String) {
    let vcf_key = format!("{}.vcf.gz", id);
    let tbi_key = format!("{}.vcf.gz.tbi", id);
    (vcf_key, tbi_key)
  }

  /// Returns [byte ranges](BytesRange) that cover all the variants
  fn get_byte_ranges_for_all_variants(
    &self,
    tbi_index: &tabix::Index,
    vcf_key: &str,
  ) -> Result<Vec<BytesRange>> {
    let get_options = GetOptions::default();
    let vcf_path = self.storage.get(vcf_key, get_options)?;
    let (mut vcf_reader, _) = self.read_vcf(vcf_path)?;

    let mut byte_ranges: Vec<BytesRange> = Vec::new();
    for reference_sequence in tbi_index.reference_sequences() {
      if let Some(metadata) = reference_sequence.metadata() {
        let start_vpos = metadata.start_position();
        let end_vpos = metadata.end_position();
        byte_ranges.push(
          BytesRange::default()
            .with_start(start_vpos.bytes_range_start())
            .with_end(end_vpos.bytes_range_end(&mut vcf_reader)),
        );
      }
    }
    Ok(BytesRange::merge_all(byte_ranges))
  }

  /// Returns [byte ranges](BytesRange) containing an specific reference sequence.
  /// Needs a Query
  fn get_byte_ranges_for_reference_name(
    &self,
    vcf_key: &str,
    reference_name: &str,
    tbi_index: &tabix::Index,
    query: &Query,
  ) -> Result<Vec<BytesRange>> {
    let get_options = GetOptions::default();
    let vcf_path = self.storage.get(vcf_key, get_options)?;
    let (mut vcf_reader, vcf_header) = Self::read_vcf(self, &vcf_path)?;
    let maybe_len = vcf_header
      .contigs()
      .get(reference_name)
      .and_then(|contig| contig.len());

    // We are assuming the order of the names and the references sequences
    // in the index is the same
    let ref_seq_index = tbi_index
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
    let byte_ranges = Self::get_byte_ranges_for_reference_sequence(
      &mut vcf_reader,
      ref_seq_index,
      tbi_index,
      seq_start,
      seq_end,
    )?;
    Ok(byte_ranges)
  }

  /// Creates a VCF reader and reads its header
  fn read_vcf<P: AsRef<Path>>(
    &self,
    path: P,
  ) -> Result<(
    vcf::Reader<noodles_bgzf::Reader<std::fs::File>>,
    vcf::Header,
  )> {
    let mut vcf_reader = File::open(&path)
      .map(bgzf::Reader::new)
      .map(vcf::Reader::new)
      .map_err(|_| HtsGetError::io_error("Reading VCF"))?;

    let vcf_header = vcf_reader
      .read_header()
      .map_err(|_| HtsGetError::io_error("Reading VCF header"))?
      .parse()
      .map_err(|_| HtsGetError::io_error("Parsing VCF header"))?;

    Ok((vcf_reader, vcf_header))
  }

  /// Returns [byte ranges](BytesRange) that cover an specific reference sequence.
  /// Needs the index of the sequence in the Tabix index
  fn get_byte_ranges_for_reference_sequence(
    vcf_reader: &mut vcf::Reader<bgzf::Reader<File>>,
    ref_seq_index: usize,
    tbi_index: &tabix::Index,
    seq_start: Option<i32>,
    seq_end: Option<i32>,
  ) -> Result<Vec<BytesRange>> {
    let seq_start = seq_start.unwrap_or(Self::MIN_SEQ_POSITION);
    let seq_end = seq_end.unwrap_or(Self::MAX_SEQ_POSITION);
    let tbi_ref_seq = tbi_index
      .reference_sequences()
      .get(ref_seq_index)
      .ok_or_else(|| HtsGetError::ParseError(String::from("Parsing TBI file")))?;

    let chunks: Vec<Chunk> = tbi_ref_seq
      .query(seq_start..=seq_end)
      .map_err(|_| HtsGetError::InvalidRange(format!("{}-{}", seq_start, seq_end)))?
      .into_iter()
      .flat_map(|bin| bin.chunks())
      .cloned()
      .collect();

    let min_offset = tbi_ref_seq.min_offset(seq_start);
    let byte_ranges = optimize_chunks(&chunks, min_offset)
      .into_iter()
      .map(|chunk| {
        BytesRange::default()
          .with_start(chunk.start().bytes_range_start())
          .with_end(chunk.end().bytes_range_end(vcf_reader))
      })
      .collect();

    Ok(BytesRange::merge_all(byte_ranges))
  }

  /// Returns a [byte range](BytesRange) that covers the header
  fn get_byte_ranges_for_header(&self, vcf_key: &str) -> Result<Vec<BytesRange>> {
    let get_options = GetOptions::default();
    let vcf_path = self.storage.get(vcf_key, get_options)?;
    let (mut vcf_reader, _) = self.read_vcf(vcf_path)?;
    let end = vcf_reader
      .virtual_position()
      .bytes_range_end(&mut vcf_reader);
    Ok(vec![BytesRange::default().with_start(0).with_end(end)])
  }

  /// Builts a [response](Response)
  fn build_response(
    &self,
    query: Query,
    vcf_key: &str,
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
          .url(&vcf_key, options)
          .map_err(HtsGetError::from)
      })
      .collect::<Result<Vec<Url>>>()?;

    let format = query.format.unwrap_or(Format::Vcf);
    Ok(Response::new(format, urls))
  }
}

#[cfg(test)]
pub mod tests {
  use crate::htsget::Headers;
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
        vec![Url::new(expected_url(&storage, filename))
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
        vec![Url::new(expected_url(&storage, filename))
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
        vec![Url::new(expected_url(&storage, filename))
          .with_headers(Headers::default().with_header("Range", "bytes=0-3367"))],
      ));
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
        vec![Url::new(expected_url(&storage, filename))
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
    test(LocalStorage::new(base_path).unwrap())
  }

  pub fn expected_url(storage: &LocalStorage, name: &str) -> String {
    format!(
      "file://{}",
      storage
        .base_path()
        .join(format!("{}.vcf.gz", name))
        .to_string_lossy()
    )
  }
}
