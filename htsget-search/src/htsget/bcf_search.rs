//! Module providing the search capability using BCF files
//!

use std::{fs::File, path::Path};

use noodles_bcf::{self as bcf};
use noodles_bgzf::{
  index::{optimize_chunks, Chunk},
  VirtualPosition,
};
use noodles_csi::{self as csi};
use noodles_vcf::{self as vcf};

use crate::{
  htsget::{Class, Format, HtsGetError, Query, Response, Result, Url},
  storage::{BytesRange, GetOptions, Storage, UrlOptions},
};

// TODO: This trait is clearly common across, **at least**, VCF, BCF and BAM
trait VirtualPositionExt {
  const MAX_BLOCK_SIZE: u64 = 65536;

  /// Get the starting bytes for a compressed BGZF block.
  fn bytes_range_start(&self) -> u64;
  /// Get the ending bytes for a compressed BGZF block.
  fn bytes_range_end(&self, reader: &mut bcf::Reader<File>) -> u64;
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
  fn bytes_range_end(&self, reader: &mut bcf::Reader<File>) -> u64 {
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
  reader: &mut bcf::Reader<File>,
) -> Option<u64> {
  reader.seek(block_position).ok()?.compressed();
  let next_block_index = loop {
    let bytes_read = reader.read_record(&mut bcf::Record::default()).ok()?;
    let actual_block_index = reader.virtual_position().compressed();
    if bytes_read == 0 || actual_block_index > block_position.compressed() {
      break actual_block_index;
    }
  };
  Some(next_block_index)
}

pub(crate) struct BcfSearch<'a, S> {
  storage: &'a S,
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

  pub fn search(&self, query: Query) -> Result<Response> {
    let (bcf_key, csi_key) = self.get_keys_from_id(query.id.as_str());

    match query.class {
      Class::Body => {
        let csi_path = self.storage.get(&csi_key, GetOptions::default())?;
        let bcf_index = csi::read(csi_path).map_err(|_| HtsGetError::io_error("Reading CSI"))?;

        let byte_ranges = match query.reference_name.as_ref() {
          None => self.get_byte_ranges_for_all_variants(&bcf_index, &bcf_key)?,
          Some(reference_name) => self.get_byte_ranges_for_reference_name(
            bcf_key.as_str(),
            reference_name,
            &bcf_index,
            &query,
          )?,
        };
        self.build_response(query, &bcf_key, byte_ranges)
      }
      Class::Header => {
        let byte_ranges = self.get_byte_ranges_for_header(bcf_key.as_str())?;
        self.build_response(query, &bcf_key, byte_ranges)
      }
    }
  }

  /// Generate a key for the storage object from an ID
  /// This may involve a more complex transformation in the future,
  /// or even require custom implementations depending on the organizational structure
  /// For now there is a 1:1 mapping to the underlying files
  fn get_keys_from_id(&self, id: &str) -> (String, String) {
    let bcf_key = format!("{}.bcf.gz", id);
    let csi_key = format!("{}.bcf.gz.csi", id);
    (bcf_key, csi_key)
  }

  /// Returns [byte ranges](BytesRange) that cover all the variants
  fn get_byte_ranges_for_all_variants(
    &self,
    csi_index: &csi::Index,
    bcf_key: &str,
  ) -> Result<Vec<BytesRange>> {
    let get_options = GetOptions::default();
    let bcf_path = self.storage.get(bcf_key, get_options)?;
    let (mut bcf_reader, _) = self.read_bcf(bcf_path)?;

    let mut byte_ranges: Vec<BytesRange> = Vec::new();
    for reference_sequence in csi_index.reference_sequences() {
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

  /// Returns [byte ranges](BytesRange) containing an specific reference sequence.
  /// Needs a Query
  fn get_byte_ranges_for_reference_name(
    &self,
    bcf_key: &str,
    reference_name: &str,
    csi_index: &csi::Index,
    query: &Query,
  ) -> Result<Vec<BytesRange>> {
    let get_options = GetOptions::default();
    let bcf_path = self.storage.get(bcf_key, get_options)?;
    let (mut bcf_reader, header) = Self::read_bcf(self, &bcf_path)?;
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
      csi_index,
      seq_start,
      seq_end,
    )?;
    Ok(byte_ranges)
  }

  /// Creates a BCF reader and reads its header
  fn read_bcf<P: AsRef<Path>>(&self, path: P) -> Result<(bcf::Reader<File>, vcf::Header)> {
    let mut bcf_reader = File::open(&path)
      .map(bcf::Reader::new)
      .map_err(|_| HtsGetError::io_error("Reading BCF"))?;
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
      .unwrap_or(VirtualPosition::from(0));
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

  /// Returns a [byte range](BytesRange) that covers the header
  fn get_byte_ranges_for_header(&self, bcf_key: &str) -> Result<Vec<BytesRange>> {
    let get_options = GetOptions::default();
    let bcf_path = self.storage.get(bcf_key, get_options)?;
    let (mut bcf_reader, _) = self.read_bcf(bcf_path)?;
    let end = bcf_reader
      .virtual_position()
      .bytes_range_end(&mut bcf_reader);
    Ok(vec![BytesRange::default().with_start(0).with_end(end)])
  }

  /// Builts a [response](Response)
  fn build_response(
    &self,
    query: Query,
    bcf_key: &str,
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
          .url(&bcf_key, options)
          .map_err(HtsGetError::from)
      })
      .collect::<Result<Vec<Url>>>()?;

    let format = query.format.unwrap_or(Format::Bcf);
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
        .join(format!("{}.bcf.gz", name))
        .to_string_lossy()
    )
  }
}
