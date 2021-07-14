//! Module providing the search capability using BCF files
//!

use std::{fs::File};

use noodles_bcf as bcf;
use noodles_bgzf::{
  index::{Chunk, optimize_chunks},
  VirtualPosition,
};
use noodles_csi::{self as csi, Index};
use noodles_vcf as vcf;

use crate::{
  htsget::{Format, HtsGetError, Query, Result},
  storage::{BytesRange, GetOptions, Storage},
};
use crate::htsget::search::{BlockPosition, Search, VirtualPositionExt};

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

impl<'a, S> Search<'a, S, csi::Index> for BcfSearch<'a, S>
  where
    S: Storage + 'a
{
  fn get_byte_ranges_for_all(&self, key: &str, index: &Index) -> Result<Vec<BytesRange>> {
    let (mut bcf_reader, _) = self.read_bcf(key)?;

    let mut byte_ranges: Vec<BytesRange> = Vec::new();
    for reference_sequence in index.reference_sequences() {
      if let Some(metadata) = reference_sequence.metadata() {
        let start_vpos = metadata.start_position();
        let end_vpos = metadata.end_position();
        byte_ranges.push(
          BytesRange::default()
            .with_start(start_vpos.bytes_range_start())
            .with_end(end_vpos.bytes_range_end(&mut bcf_reader)),
        );
      }
    }
    Ok(BytesRange::merge_all(byte_ranges))
  }

  fn get_byte_ranges_for_reference_name(&self, key: &str, reference_name: &str, index: &Index, query: &Query) -> Result<Vec<BytesRange>> {
    let (mut bcf_reader, header) = Self::read_bcf(self, key)?;
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
    let byte_ranges = Self::get_byte_ranges_for_reference_sequence(
      &mut bcf_reader,
      ref_seq_index,
      index,
      seq_start,
      seq_end,
    )?;
    Ok(byte_ranges)
  }

  fn read_index(&self, key: &str) -> Result<Index> {
    let csi_path = self.storage.get(&key, GetOptions::default())?;
    csi::read(csi_path).map_err(|_| HtsGetError::io_error("Reading CSI"))
  }

  fn get_keys_from_id(&self, id: &str) -> (String, String) {
    let bcf_key = format!("{}.bcf", id);
    let csi_key = format!("{}.bcf.csi", id);
    (bcf_key, csi_key)
  }

  fn get_byte_ranges_for_header(&self, key: &str) -> Result<Vec<BytesRange>> {
    let (mut bcf_reader, _) = self.read_bcf(key)?;
    let end = bcf_reader
      .virtual_position()
      .bytes_range_end(&mut bcf_reader);
    Ok(vec![BytesRange::default().with_start(0).with_end(end)])
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
  const MIN_SEQ_POSITION: i32 = 1; // 1-based
  const MAX_SEQ_POSITION: i32 = (1 << 29) - 1; // see https://github.com/zaeleus/noodles/issues/25#issuecomment-868871298

  pub fn new(storage: &'a S) -> Self {
    Self { storage }
  }

  /// Creates a BCF reader and reads its header
  fn read_bcf(&self, key: &str) -> Result<(bcf::Reader<File>, vcf::Header)> {
    let mut bcf_reader = self.get_reader(key, "Reading BCF", bcf::Reader::new)?;
    let _ = bcf_reader.read_file_format()?;

    let header = bcf_reader
      .read_header()
      .map_err(|_| HtsGetError::io_error("Reading BCF header"))?
      .parse::<vcf::Header>()
      .map_err(|_| HtsGetError::io_error("Parsing BCF header"))?;

    Ok((bcf_reader, header))
  }

  /// Returns [byte ranges](BytesRange) that cover an specific reference sequence.
  /// Needs the index of the sequence in the CSI index
  fn get_byte_ranges_for_reference_sequence(
    bcf_reader: &mut bcf::Reader<File>,
    ref_seq_index: usize,
    csi_index: &csi::Index,
    seq_start: Option<i32>,
    seq_end: Option<i32>,
  ) -> Result<Vec<BytesRange>> {
    let seq_start = seq_start.unwrap_or(Self::MIN_SEQ_POSITION);
    let seq_end = seq_end.unwrap_or(Self::MAX_SEQ_POSITION);
    let csi_ref_seq = csi_index
      .reference_sequences()
      .get(ref_seq_index)
      .ok_or_else(|| HtsGetError::ParseError(String::from("Parsing CSI file")))?;

    let chunks: Vec<Chunk> = csi_ref_seq
      .query(
        csi_index.min_shift(),
        csi_index.depth(),
        seq_start as i64..=seq_end as i64,
      )
      .map_err(|_| HtsGetError::InvalidRange(format!("{}-{}", seq_start, seq_end)))?
      .into_iter()
      .flat_map(|bin| bin.chunks())
      .cloned()
      .collect();

    let min_offset = csi_ref_seq
      .metadata()
      .map(|metadata| metadata.start_position())
      .unwrap_or_else(|| VirtualPosition::from(0));
    let byte_ranges = optimize_chunks(&chunks, min_offset)
      .into_iter()
      .map(|chunk| {
        BytesRange::default()
          .with_start(chunk.start().bytes_range_start())
          .with_end(chunk.end().bytes_range_end(bcf_reader))
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
