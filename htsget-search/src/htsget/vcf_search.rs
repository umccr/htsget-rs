//! Module providing the search capability using VCF files
//!

use std::{fs::File, path::Path, io::{BufReader}};

use noodles_vcf::{self as vcf};
use noodles_tabix::{self as tabix};
use noodles_bgzf::{self as bgzf};

use crate::{
    htsget::{Class, Format, HtsGetError, Query, Response, Result, Url},
    storage::{BytesRange, GetOptions, Storage, UrlOptions},
};

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
   
    /// TODO: Refer to https://github.com/zaeleus/noodles/commit/a00901697d0fafa1595981eff00488aa305e1429
    pub fn search(&self, query: Query) -> Result<Response> {
      let vcf_key = self.get_keys_from_id(query.id.as_str());
  
      match query.class {
        None | Some(Class::Body) => {
          let vcf_path = self.storage.get(&vcf_key, GetOptions::default())?;
          let vcf_reader = self.read_vcf(vcf_path);
        }
        Some(Class::Header) => {
          let vcf_path = self.storage.get(&vcf_key, GetOptions::default())?;
          let vcf_reader = self.read_vcf(vcf_path);
        }
      }
      
      let byte_ranges = match query.reference_name.as_ref() {
        Some(header) => todo!(),
        Some(_) => todo!(),
        None => todo!() //self.get_byte_ranges_for_all_records(vcf_reader?.0)?
      };

          // TODO:
          // 1. Read records directly, header might not be needed
          // 2. Get POS for the records of the ID being queried
          // 3. Return an appropriate byte_range for the repeated IDs (CHROMS)
  
          // let byte_ranges = match query.reference_name.as_ref() {
          //   None => self.get_byte_ranges_for_all_records(vcf_key.as_str())?,
          //   Some(reference_name) => self.get_byte_ranges_for_reference_name(
          //     vcf_key.as_str(),
          //     reference_name,
          //     &query,
          //   )?,
          // };
  
        self.build_response(query, &vcf_key, byte_ranges)
    }

    fn get_keys_from_id(&self, id: &str) -> String {
      let vcf_key = format!("{}.vcf.gz", id); // TODO: allow uncompressed, plain, .vcf files
      vcf_key
    }

    fn get_byte_ranges_for_all_records(
      &self,
      reader: vcf::Reader<BufReader<File>>,
    ) -> Result<Vec<BytesRange>> {
       let byte_ranges: Vec<BytesRange> = Vec::new();
       Ok(byte_ranges)
    }

    fn read_vcf<P: AsRef<Path>>(&self, path: P) -> Result<(vcf::Reader<BufReader<File>>, vcf::Header, tabix::Index)> {
      let mut vcf_reader = File::open(path)
        .map(bgzf::Reader::new) 
        .map(vcf::Reader::new)
        .map_err(|_| HtsGetError::io_error("Reading VCF"))?;
 
      let vcf_header = vcf_reader
        .read_header()
        .map_err(|_| HtsGetError::io_error("Reading VCF header"))?
        .parse()
        .map_err(|_| HtsGetError::io_error("Parsing VCF header"))?;
      
      let vcf_index = tabix::read(path); //+".tbi" is typical vcf index extension, but should be flexible accepting other fnames
      
      Ok((vcf_reader, vcf_header, vcf_index))
    }

    fn build_response(
      &self,
      query: Query,
      bam_key: &str,
      byte_ranges: Vec<BytesRange>,
    ) -> Result<Response> {
      todo!()
    }  
}