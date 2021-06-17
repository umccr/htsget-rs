use serde::{Serialize, Serializer};
use std::fs::File;
use std::io::Result;
use std::path::Path;

use noodles_bgzf::{self as bgzf, VirtualPosition};
use noodles_tabix::{self as tabix};
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
      while last_block < end {
        reader.read_record(&mut String::new())?;
        if reader.virtual_position().compressed() != last_block.compressed() {
          blocks.push(Block {
            start: last_block,
            end: reader.virtual_position(),
          });
          last_block = reader.virtual_position();
        }
      }
      chunks.push(Chunk { start, end, blocks })
    }
    ref_seqs.push(RefSeq {
      name: name.clone(),
      index: index,
      start: ref_seq
        .metadata()
        .map(|metadata| metadata.start_position())
        .unwrap_or(VirtualPosition::from(0)),
      end: ref_seq
        .metadata()
        .map(|metadata| metadata.end_position())
        .unwrap_or(VirtualPosition::from(0)),
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
