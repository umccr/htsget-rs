//! Module providing the search capability using VCF files
//!

use std::{fs::File, path::Path};

use noodles_bgzf::{
  self as bgzf,
  index::{Chunk, optimize_chunks},
  VirtualPosition,
};
use noodles_tabix::{self as tabix, Index};
use noodles_vcf as vcf;

use crate::{
  htsget::{Format, HtsGetError, Query, Result},
  storage::{BytesRange, GetOptions, Storage},
};
use crate::htsget::search::{BlockPosition, Search, VirtualPositionExt};

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

impl<'a, S> Search<'a, S, Index> for VcfSearch<'a, S>
  where
    S: Storage + 'a
{
  fn get_byte_ranges_for_all(&self, key: &str, index: &Index) -> Result<Vec<BytesRange>> {
    let get_options = GetOptions::default();
    let vcf_path = self.storage.get(key, get_options)?;
    let (mut vcf_reader, _) = self.read_vcf(vcf_path)?;

    let mut byte_ranges: Vec<BytesRange> = Vec::new();
    for reference_sequence in index.reference_sequences() {
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

  fn get_byte_ranges_for_reference_name(&self, key: &str, reference_name: &str, index: &Index, query: &Query) -> Result<Vec<BytesRange>> {
    let get_options = GetOptions::default();
    let vcf_path = self.storage.get(key, get_options)?;
    let (mut vcf_reader, vcf_header) = Self::read_vcf(self, &vcf_path)?;
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
    let byte_ranges = Self::get_byte_ranges_for_reference_sequence(
      &mut vcf_reader,
      ref_seq_index,
      index,
      seq_start,
      seq_end,
    )?;
    Ok(byte_ranges)
  }

  fn read_index(&self, key: &str) -> Result<Index> {
    let tbi_path = self.storage.get(&key, GetOptions::default())?;
    tabix::read(tbi_path).map_err(|_| HtsGetError::io_error("Reading TBI"))
  }

  fn get_keys_from_id(&self, id: &str) -> (String, String) {
    let vcf_key = format!("{}.vcf.gz", id);
    let tbi_key = format!("{}.vcf.gz.tbi", id);
    (vcf_key, tbi_key)
  }

  fn get_byte_ranges_for_header(&self, key: &str) -> Result<Vec<BytesRange>> {
    let get_options = GetOptions::default();
    let vcf_path = self.storage.get(key, get_options)?;
    let (mut vcf_reader, _) = self.read_vcf(vcf_path)?;
    let end = vcf_reader
      .virtual_position()
      .bytes_range_end(&mut vcf_reader);
    Ok(vec![BytesRange::default().with_start(0).with_end(end)])
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
  const MIN_SEQ_POSITION: i32 = 1; // 1-based
  const MAX_SEQ_POSITION: i32 = (1 << 29) - 1; // see https://github.com/zaeleus/noodles/issues/25#issuecomment-868871298

  pub fn new(storage: &'a S) -> Self {
    Self { storage }
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
