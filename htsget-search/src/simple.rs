use std::{
  fs::File,
  path::{Path, PathBuf},
};

use noodles::Region;
use noodles_bam::{self as bam, bai};
use noodles_bgzf::VirtualPosition;
use noodles_sam::{self as sam};
use sam::header::ReferenceSequences;

use crate::htsget::{Format, HtsGet, HtsGetError, Query, Response};

pub struct SimpleHtsGet {
  base_path: PathBuf,
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
    match query.format {
      Some(Format::BAM) | None => self.search_bam(query),
      Some(format) => Err(HtsGetError::unsupported_format(format)),
    }
  }
}

impl SimpleHtsGet {
  fn search_bam(&self, query: Query) -> Result<Response, HtsGetError> {
    // TODO check class, for now assuming it is None or "body"

    let bam_path = self.get_bam_path_from_id(query.id.as_str())?;
    let bai_path = bam_path.with_extension(".bai");
    let index = bai::read(bai_path).map_err(|_| HtsGetError::io_error("Reading BAI"))?;

    let positions = match query.reference_name.as_ref() {
      None => {
        let mut positions: Vec<(VirtualPosition, VirtualPosition)> = Vec::new();
        for reference_sequence in index.reference_sequences() {
          if let Some(metadata) = reference_sequence.metadata() {
            // TODO Report to the noodles author that the call to reference_sequence.min_offset(0) will panic, can we avoid that panic?
            let start_vpos = reference_sequence.min_offset(1);
            let end_vpos = metadata.end_position();
            positions.push((start_vpos, end_vpos));
          }
        }
        positions
      }
      Some(reference_name) if reference_name.as_str() == "*" => {
        let mut positions: Vec<(VirtualPosition, VirtualPosition)> = Vec::new();
        for reference_sequence in index.reference_sequences() {
          if let Some(metadata) = reference_sequence.metadata() {
            // TODO Ask to the noodles author whether metadata.start_position will include unmapped reads or not
            let start_vpos = metadata.start_position();
            let end_vpos = metadata.end_position();
            positions.push((start_vpos, end_vpos));
          }
        }
        positions
      }
      Some(reference_name) => {
        let bam_header = Self::read_bam_header(&bam_path)?;
        let reference_sequences = bam_header.reference_sequences();
        match reference_sequences.get(reference_name) {
          None => Err(HtsGetError::not_found(format!(
            "Reference name not found: {}",
            reference_name
          ))),
          Some(reference_sequence) => {
            // let region = self.get_region_from_query(&query, reference_sequences)?;
            // let q = bam_reader.query(reference_sequences, &index, &region)
            //   .map_err(|_| HtsGetError::IOError("Querying BAM".to_string()))?;
            unimplemented!()
          }
        }?
      }
    };

    // TODO build the Response from the vector of virtual positions
    unimplemented!()
  }

  fn get_bam_path_from_id(&self, id: &str) -> Result<PathBuf, HtsGetError> {
    let path = self
      .base_path
      .join(id)
      .with_extension(".bam")
      .canonicalize()
      .map_err(|_| HtsGetError::InvalidInput("Malformed query 'id'".to_string()))?;

    if !path.starts_with(&self.base_path) {
      Err(HtsGetError::InvalidInput(
        "Malformed query 'id'".to_string(),
      ))
    } else {
      Ok(path)
    }
  }

  fn read_bam_header<P: AsRef<Path>>(path: P) -> Result<sam::Header, HtsGetError> {
    let mut bam_reader = File::open(path.as_ref())
      .map(bam::Reader::new)
      .map_err(|_| HtsGetError::IOError("Reading BAM".to_string()))?;

    bam_reader
      .read_header()
      .map_err(|_| HtsGetError::IOError("Reading BAM".to_string()))?
      .parse()
      .map_err(|_| HtsGetError::IOError("Reading BAM".to_string()))
  }

  fn get_region_from_query(
    &self,
    query: &Query,
    reference_sequences: &ReferenceSequences,
  ) -> Result<Region, HtsGetError> {
    let raw_region = match query.reference_name.as_ref() {
      None => ".".to_string(),
      Some(reference_name) => match query.start.as_ref() {
        None => reference_name.clone(),
        Some(start) => match query.end.as_ref() {
          None => format!("{}:{}", *reference_name, *start),
          Some(end) => format!("{}:{}-{}", *reference_name, *start, *end),
        },
      },
    };

    Region::from_str_reference_sequences(raw_region.as_str(), reference_sequences).map_err(|_| {
      HtsGetError::InvalidInput(format!("Malformed reference sequences: {}", &raw_region))
    })
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
