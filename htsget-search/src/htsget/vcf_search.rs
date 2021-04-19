//! Module providing the search capability using VCF files
//!

use std::{fs::File, path::Path};


use noodles_vcf::{self as vcf};

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
    
    pub fn search(&self, query: Query) -> Result<Response> {
        let vcf_key = self.get_keys_from_id(query.id.as_str());
    
        match query.class {
          None | Some(Class::Body) => {
            let vcf_path = self.storage.get(&vcf_key, GetOptions::default())?;
            let vcf_header = vcf::Reader::read_header(vcf_path).map_err(|_| HtsGetError::io_error("Reading VCF"))?;

            // TODO:
            // 1. Read records directly, header might not be needed
            // 2. Get POS for the records of the ID being queried
            // 3. Return an appropriate byte_range for the repeated IDs (CHROMS)
    
            self.build_response(query, &vcf_key, _byte_ranges)
          }
          Some(Class::Header) => {
            let byte_ranges = self.get_byte_ranges_for_header();
            self.build_response(query, &bam_key, byte_ranges)
          }
        }
      }

      fn get_keys_from_id(&self, id: &str) -> String {
        let vcf_key = format!("{}.vcf", id);
        vcf_key
      }

      fn get_byte_ranges_for_all_records(
        &self,
        vcf_key: &str,
      ) -> Result<Vec<BytesRange>> {
          todo!()
      }    
}    