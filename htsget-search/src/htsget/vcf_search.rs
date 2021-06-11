//! Module providing the search capability using VCF files
//!

use std::convert::TryInto;
use std::{fs::File, path::Path}; //, io::{BufReader}};

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
  fn bytes_range_end(&self) -> u64;
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
  /// The simple solution goes through adding the maximum BGZF block size,
  /// so we don't loose any read (although adding extra unneeded reads to the query results).
  fn bytes_range_end(&self) -> u64 {
    self.compressed() + Self::MAX_BLOCK_SIZE
  }

  fn to_string(&self) -> String {
    format!("{}/{}", self.compressed(), self.uncompressed())
  }
}

pub(crate) struct VCFSearch<'a, S> {
  storage: &'a S,
}

impl<'a, S> VCFSearch<'a, S>
where
  S: Storage + 'a,
{
  pub fn new(storage: &'a S) -> Self {
    Self { storage }
  }

  pub fn search(&self, query: Query) -> Result<Response> {
    let (vcf_key, _tbi_key) = self.get_keys_from_id(query.id.as_str());

    match query.class {
      Class::Body => {
        let tbi_path = self.storage.get(&vcf_key, GetOptions::default())?; // TODO: Be more flexible/resilient with index files, do not just assume `.tbi` within the same directory
        let vcf_index = tabix::read(tbi_path).map_err(|_| HtsGetError::io_error("Reading TBI"))?;

        let byte_ranges = match query.reference_name.as_ref() {
          None => self.get_byte_ranges_for_all_variants(vcf_key.as_str(), &vcf_index)?,
          Some(reference_name) if reference_name.as_str() == "*" => {
            self.get_byte_ranges_for_all_variants(vcf_key.as_str(), &vcf_index)?
          }
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
        let byte_ranges = self.get_byte_ranges_for_header();
        self.build_response(query, &vcf_key, byte_ranges)
      }
    }
  }

  fn get_byte_ranges_for_header(&self) -> Vec<BytesRange> {
    vec![BytesRange::default().with_start(0).with_end(4096)] // XXX: Check spec
  }

  fn get_byte_ranges_for_reference_name(
    &self,
    vcf_key: &str,
    reference_name: &str,
    tbi_index: &tabix::Index,
    query: &Query,
  ) -> Result<Vec<BytesRange>> {
    let get_options = GetOptions::default().with_max_length(4096); // XXX: Read spec, what's the max length for this?
    let vcf_path = self.storage.get(vcf_key, get_options)?;
    let (_, vcf_header, _) = Self::read_vcf(self, &vcf_path)?;
    let maybe_vcf_ref_seq = vcf_header.sample_names().get_full(reference_name);

    let byte_ranges = match maybe_vcf_ref_seq {
      None => Err(HtsGetError::not_found(format!(
        "Reference name not found: {}",
        reference_name
      ))),
      Some((vcf_ref_seq_idx, vcf_ref_seq)) => {
        let seq_start = query.start.map(|start| start as i32);
        let seq_end = query.end.map(|end| end as i32);
        Self::get_byte_ranges_for_reference_sequence(
          vcf_ref_seq,
          vcf_ref_seq_idx,
          tbi_index,
          seq_start,
          seq_end,
        )
      }
    }?;
    Ok(byte_ranges)
  }

  fn get_byte_ranges_for_reference_sequence(
    vcf_ref_seq: &vcf::header::Samples,
    vcf_ref_seq_idx: usize,
    tbi_index: &tabix::Index,
    seq_start: Option<i32>,
    seq_end: Option<i32>,
  ) -> Result<Vec<BytesRange>> {
    let seq_start = seq_start.unwrap_or(4096 as i32); // XXX: Check spec
    let seq_end = seq_end.unwrap_or_else(|| vcf_ref_seq.len().try_into().unwrap()); // XXX: Revisit
    let tbi_ref_seq = tbi_index
      .reference_sequences()
      .get(vcf_ref_seq_idx)
      .ok_or_else(|| {
        HtsGetError::not_found(format!(
          "Reference not found in the TBI file: {} ({})",
          vcf_ref_seq, vcf_ref_seq_idx
        ))
      })?;

    let chunks: Vec<Chunk> = tbi_ref_seq
      .query(seq_start, seq_end)
      .into_iter()
      .flat_map(|bin| bin.chunks())
      .cloned()
      .collect();

    //let min_offset = vcf_ref_seq.

    let byte_ranges = optimize_chunks(&chunks, min_offset)
      .into_iter()
      .map(|chunk| {
        BytesRange::default()
          .with_start(chunk.start().bytes_range_start())
          .with_end(chunk.end().bytes_range_end())
      })
      .collect();

    Ok(BytesRange::merge_all(byte_ranges))
  }

  fn get_byte_ranges_for_all_variants(
    &self,
    vcf_key: &str,
    tbi_index: &tabix::Index,
  ) -> Result<Vec<BytesRange>> {
    let mut byte_ranges: Vec<BytesRange> = Vec::new();
    for reference_sequence in tbi_index.reference_sequences() {
      if let Some(metadata) = reference_sequence.metadata() {
        let start_vpos = metadata.start_position();
        let end_vpos = metadata.end_position();
        byte_ranges.push(
          BytesRange::default()
            .with_start(start_vpos.bytes_range_start())
            .with_end(end_vpos.bytes_range_end()),
        );
      }
    }

    let unmapped_byte_ranges = self.get_byte_ranges_for_all_variants(vcf_key, tbi_index)?;
    byte_ranges.extend(unmapped_byte_ranges.into_iter());
    Ok(BytesRange::merge_all(byte_ranges))
  }

  fn get_keys_from_id(&self, id: &str) -> (String, String) {
    let vcf_key = format!("{}.vcf.gz", id); // TODO: allow uncompressed, plain, .vcf files
    let tbi_key = format!("{}.vcf.gz.tbi", id);
    (vcf_key, tbi_key)
  }

  fn read_vcf<P: AsRef<Path>>(
    &self,
    path: P,
  ) -> Result<(
    vcf::Reader<noodles_bgzf::Reader<std::fs::File>>,
    vcf::Header,
    tabix::Index,
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

    let vcf_index = tabix::read(&path).map_err(|_| HtsGetError::io_error("Reading index"))?; //+".tbi" is typical vcf index extension, but should be flexible accepting other fnames

    Ok((vcf_reader, vcf_header, vcf_index))
  }

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
