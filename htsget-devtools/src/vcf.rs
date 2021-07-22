use serde::{Serialize, Serializer};
use std::fs::File;
use std::io::Result;
use std::path::Path;

use noodles::bgzf;
use noodles::bgzf::VirtualPosition;
use noodles::csi::{BinningIndex, BinningIndexReferenceSequence};
use noodles::tabix;
use noodles::vcf;

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

pub fn vcf_blocks<P: AsRef<Path>>(path: P) -> Result<Vec<RefSeq>> {
  let index = tabix::read(path.as_ref().with_extension("gz.tbi"))?;
  let mut reader = File::open(path.as_ref())
    .map(bgzf::Reader::new)
    .map(vcf::Reader::new)?;

  let _ = reader.read_header()?.parse::<vcf::Header>().unwrap();

  let mut ref_seqs = Vec::new();

  for (index, (name, ref_seq)) in index
    .reference_sequence_names()
    .iter()
    .zip(index.reference_sequences())
    .enumerate()
  {
    let mut chunks = Vec::new();

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
        let mut record = String::new();
        let previous_pos = reader.virtual_position();
        let bytes_read = reader.read_record(&mut record)?;
        if bytes_read == 0 {
          break; //EOF
        }
        let record: vcf::Record = record.parse().unwrap();
        if previous_pos < end {
          seq_start = seq_start.min(record.position().into());
          seq_end = seq_end.max(record.position().into());
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
      name: name.clone(),
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
