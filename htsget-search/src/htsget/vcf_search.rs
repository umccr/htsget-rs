//! Module providing the search capability using VCF files
//!

use std::str::FromStr;
use std::{fs::File, path::Path};//, io::{BufReader}};

use noodles_core::{Region};
use noodles_vcf::{self as vcf};
use noodles_tabix::{self as tabix};
use noodles_bgzf::{self as bgzf, VirtualPosition};

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
    const DEFAULT_BAM_HEADER_LENGTH: u64 = 1024 * 1024; // TODO find a number that makes more sense

    pub fn new(storage: &'a S) -> Self {
        Self { storage }
    }
   
    /// TODO: The (VCF)::Reader::query method is mirrored into the BAM implementation, reducing code duplication
    pub fn search(&self, query: Query) -> Result<Response> {
      let (vcf_key, tbi_key) = self.get_keys_from_id(query.id.as_str());
  
      match query.class {
        None | Some(Class::Body) => {
          let _tbi_path = self.storage.get(&vcf_key, GetOptions::default())?; // TODO: Be more flexible/resilient with index files, do not just assume `.tbi` within the same directory 
          //let vcf_index = tabix::read(tbi_path).map_err(|_| HtsGetError::io_error("Reading TBI"))?;
          let (vcf_reader, vcf_header, vcf_index)  = self.read_vcf(vcf_key)?;

          let byte_ranges = match query.reference_name.as_ref() {
            None => vcf_reader.query(&vcf_index, &Region::All),
            Some(reference_name) if reference_name.as_str() == "*" => {
              vcf_reader.query(&vcf_index, &Region::Unmapped)
            }
            // TODO: Map this Noodles-core::ParseError into HtsGetError
            Some(reference_name) => vcf_reader.query(&vcf_index, &Region::from_str(reference_name.as_str())?)              
          };
          // TODO: Turn the result from vcf_reader.query into Vec<ByteRanges>`
          self.build_response(query, &vcf_key, byte_ranges?)
        }
        Some(Class::Header) => {
          let byte_ranges = todo!();
          self.build_response(query, &vcf_key, byte_ranges)
        }
      }
    }

    fn get_keys_from_id(&self, id: &str) -> (String, String) {
      let vcf_key = format!("{}.vcf.gz", id); // TODO: allow uncompressed, plain, .vcf files
      let tbi_key = format!("{}.vcf.gz.tbi", id);
      (vcf_key, tbi_key)
    }

    fn read_vcf<P: AsRef<Path>>(&self, path: P) -> Result<(vcf::Reader<noodles_bgzf::Reader<std::fs::File>>, vcf::Header, tabix::Index)> {
      let mut vcf_reader = File::open(&path)
        .map(bgzf::Reader::new) 
        .map(vcf::Reader::new)
        .map_err(|_| HtsGetError::io_error("Reading VCF"))?;
 
      let vcf_header = vcf_reader
        .read_header()
        .map_err(|_| HtsGetError::io_error("Reading VCF header"))?
        .parse()
        .map_err(|_| HtsGetError::io_error("Parsing VCF header"))?;
      
     let vcf_index = tabix::read(&path)
        .map_err(|_| HtsGetError::io_error("Reading index"))?; //+".tbi" is typical vcf index extension, but should be flexible accepting other fnames
      
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
          let options = match query.class.as_ref() {
            None => UrlOptions::default().with_range(range),
            Some(class) => UrlOptions::default()
              .with_range(range)
              .with_class(class.clone()),
          };
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