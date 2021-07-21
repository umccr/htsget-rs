use serde::{Serialize, Serializer};
use std::fs::File;
use std::io::Result;
use std::path::Path;

use noodles_bcf::{self as bcf};
use noodles_bgzf::VirtualPosition;
use noodles_csi::{self as csi, BinningIndex, BinningIndexReferenceSequence};
use noodles_vcf::{self as vcf};

#[derive(Debug, Serialize)]
pub struct RefSeq {
  name: String,
  index: usize,
  #[serde(serialize_with = "serde_virtual_position")]
  start: VirtualPosition,
  #[serde(serialize_with = "serde_virtual_position")]
  end: VirtualPosition,
  chunks: Vec<Chunk>,
}

#[derive(Debug, Serialize)]
pub struct Chunk {
  #[serde(serialize_with = "serde_virtual_position")]
  start: VirtualPosition,
  #[serde(serialize_with = "serde_virtual_position")]
  end: VirtualPosition,
  blocks: Vec<Block>,
}

#[derive(Debug, Serialize)]
pub struct Block {
  #[serde(serialize_with = "serde_virtual_position")]
  start: VirtualPosition,
  #[serde(serialize_with = "serde_virtual_position")]
  end: VirtualPosition,
  seq_start: i32,
  seq_end: i32,
}

pub fn bcf_blocks<P: AsRef<Path>>(path: P) -> Result<Vec<RefSeq>> {
  let mut reader = File::open(path.as_ref()).map(bcf::Reader::new)?;
  let index = csi::read(path.as_ref().with_extension("bcf.csi"))?;

  let _ = reader.read_file_format()?;
  let header = reader.read_header()?.parse::<vcf::Header>().unwrap();

  let mut ref_seqs = Vec::new();

  for (index, ref_seq) in index.reference_sequences().iter().enumerate() {
    let mut chunks = Vec::new();
    let mut id = 0;

    for (start, end) in ref_seq
      .bins()
      .iter()
      .flat_map(|bin| bin.chunks())
      .map(|chunk| (chunk.start(), chunk.end()))
    {
      let mut blocks = Vec::new();
      reader.seek(start)?;
      let mut last_block = start;
      let mut seq_start = i32::MAX;
      let mut seq_end = 0;
      while last_block < end {
        let mut record = bcf::Record::default();
        let previous_pos = reader.virtual_position();
        let bytes_read = reader.read_record(&mut record)?;
        if bytes_read == 0 {
          break; //EOF
        }
        if previous_pos < end {
          id = record.chromosome_id().unwrap_or(id);
          seq_start = seq_start.min(record.position().unwrap().into());
          seq_end = seq_end.max(record.position().unwrap().into());
        }
        if reader.virtual_position().compressed() != last_block.compressed() {
          blocks.push(Block {
            start: last_block,
            end: reader.virtual_position(),
            seq_start,
            seq_end,
          });
          last_block = reader.virtual_position();
        }
      }
      chunks.push(Chunk { start, end, blocks })
    }
    ref_seqs.push(RefSeq {
      name: header.contigs()[id as usize].id().to_string(),
      index,
      start: ref_seq
        .metadata()
        .map(|metadata| metadata.start_position())
        .unwrap_or_else(|| VirtualPosition::from(0)),
      end: ref_seq
        .metadata()
        .map(|metadata| metadata.end_position())
        .unwrap_or_else(|| VirtualPosition::from(0)),
      chunks,
    })
  }
  Ok(ref_seqs)
}

pub fn serde_virtual_position<S>(
  vpos: &VirtualPosition,
  serializer: S,
) -> core::result::Result<S::Ok, S::Error>
where
  S: Serializer,
{
  let s = format!("{}/{}", vpos.compressed(), vpos.uncompressed());
  serializer.serialize_str(s.as_str())
}
