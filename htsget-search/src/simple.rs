use std::{fs::File, path::{Path, PathBuf}};

use noodles::Region;
use noodles_bam::{self as bam, bai};
use noodles_sam::{self as sam};
use sam::header::ReferenceSequences;

use crate::htsget::{HtsGet, Query, Response, HtsGetError};

pub struct SimpleHtsGet {
  base_path: PathBuf
}

impl SimpleHtsGet {
  pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
    let base_path: &Path = base_path.as_ref();
    Self {
      base_path: base_path.to_path_buf(),
    }
  }
}

impl HtsGet for SimpleHtsGet {
  fn search(&self, query: Query) -> Result<Response, HtsGetError> {
    let bam_path = self.get_bam_path_from_id(query.id.as_str())?;
    let bai_path = bam_path.with_extension(".bai");
    
    let mut bam_reader = File::open(&bam_path)
      .map(bam::Reader::new)
      .map_err(|_| HtsGetError::IOError("Reading BAM".to_string()))?;

    let header: sam::Header = bam_reader.read_header()
      .map_err(|_| HtsGetError::IOError("Reading BAM".to_string()))?
      .parse()
      .map_err(|_| HtsGetError::IOError("Reading BAM".to_string()))?;

    let reference_sequences = header.reference_sequences();

    let index = bai::read(bai_path)
      .map_err(|_| HtsGetError::IOError("Reading BAI".to_string()))?;

    let region = self.get_region_from_query(&query, reference_sequences)?;

    let q = bam_reader.query(reference_sequences, &index, &region)
      .map_err(|_| HtsGetError::IOError("Querying BAM".to_string()))?;
    
    unimplemented!()
  }
}

impl SimpleHtsGet {
  fn get_bam_path_from_id(&self, id: &str) -> Result<PathBuf, HtsGetError> {
    let path = self.base_path
      .join(id)
      .with_extension(".bam")
      .canonicalize()
      .map_err(|_| HtsGetError::InvalidInput("Malformed query 'id'".to_string()))?;

    if !path.starts_with(&self.base_path) {
      Err(HtsGetError::InvalidInput("Malformed query 'id'".to_string()))
    }
    else {
      Ok(path)
    }
  }

  fn get_region_from_query(&self, query: &Query, reference_sequences: &ReferenceSequences) -> Result<Region, HtsGetError> {
    let raw_region = match query.reference_name.as_ref() {
      None => ".".to_string(),
      Some(reference_name) => {
        match query.start.as_ref() {
          None => reference_name.clone(),
          Some(start) => {
            match query.end.as_ref() {
              None => format!("{}:{}", *reference_name, *start),
              Some(end) => format!("{}:{}-{}", *reference_name, *start, *end),
            }
          }
        }
      }
    };

    Region::from_str_reference_sequences(raw_region.as_str(), reference_sequences)
      .map_err(|_| HtsGetError::InvalidInput(format!("Malformed reference sequences: {}", &raw_region)))
  }
}


#[cfg(test)]
mod tests {

  use super::*;

  #[test]
  fn search_() {
    // TODO determine root path through cargo env vars
    let htsget = SimpleHtsGet::new("../data");

  }
}