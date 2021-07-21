use serde::{Serialize, Serializer};
use std::convert::TryFrom;
use std::fs::File;
use std::io::Result;
use std::{collections::HashSet, path::Path};

use bam::Record;
use noodles_bam::{self as bam, bai};
use noodles_bgzf::VirtualPosition;
use noodles_csi::{BinningIndex, BinningIndexReferenceSequence};
use noodles_sam::{self as sam};

#[derive(Debug, Serialize)]
pub struct RefSeq {
  name: String,
  index: usize,
  len: usize,
  #[serde(serialize_with = "serde_virtual_position")]
  start: VirtualPosition,
  #[serde(serialize_with = "serde_virtual_position")]
  end: VirtualPosition,
  seq_start: u64,
  seq_end: u64,
  blocks: Vec<Block>,
}

#[derive(Debug, Serialize)]
pub struct Block {
  start: u64,
  end: u64,
  seq_start: u64,
  seq_end: u64,
  mapped_count: usize,
  unmapped_count: usize,
}

pub fn bam_blocks<P: AsRef<Path>>(path: P) -> Result<Vec<RefSeq>> {
  let index = bai::read(path.as_ref().with_extension("bam.bai"))?;
  let mut reader = File::open(path.as_ref()).map(bam::Reader::new)?;
  let header = reader.read_header()?.parse::<sam::Header>().unwrap();

  let mut ref_seqs: Vec<RefSeq> = Vec::with_capacity(header.reference_sequences().len());

  let joined_ref_seqs = header
    .reference_sequences()
    .into_iter()
    .zip(index.reference_sequences().iter())
    .enumerate();

  for (idx, ((ref_seq_name, hdr_ref_seq), idx_ref_seq)) in joined_ref_seqs {
    if let Some(metadata) = idx_ref_seq.metadata() {
      let blocks: HashSet<u64> = idx_ref_seq
        .bins()
        .iter()
        .flat_map(|bin| bin.chunks().iter())
        .flat_map(|chunk| vec![chunk.start(), chunk.end()])
        .map(|vpos| vpos.compressed())
        .collect();
      let mut blocks: Vec<u64> = blocks.into_iter().collect();
      blocks.sort_unstable();

      let intervals: Vec<(u64, u64)> = blocks
        .iter()
        .take(blocks.len() - 1)
        .zip(blocks.iter().skip(1))
        .map(|(start, end)| (*start, *end))
        .collect();

      let mut ref_seq_start = u64::MAX;
      let mut ref_seq_end = u64::MIN;
      let mut blocks = Vec::new();
      let mut record: Record = Record::default();
      for (start, end) in intervals {
        let mut seq_start = u64::MAX;
        let mut seq_end = u64::MIN;
        let mut mapped_count = 0usize;
        let mut unmapped_count = 0usize;
        reader.seek(VirtualPosition::try_from((start, 0u16)).unwrap())?;
        while reader.virtual_position().compressed() < end {
          reader.read_record(&mut record)?;
          if let Some(position) = record.position() {
            mapped_count += 1;
            let pos = i32::from(position) as u64;
            seq_start = u64::min(seq_start, pos);
            seq_end = u64::max(seq_end, pos);
          } else {
            unmapped_count += 1;
          }
        }
        if seq_start < seq_end {
          ref_seq_start = u64::min(ref_seq_start, seq_start);
          ref_seq_end = u64::max(ref_seq_end, seq_end);
          blocks.push(Block {
            start,
            end,
            seq_start,
            seq_end,
            mapped_count,
            unmapped_count,
          })
        }
      }

      let rs = RefSeq {
        index: idx,
        len: hdr_ref_seq.len() as usize,
        name: ref_seq_name.clone(),
        start: metadata.start_position(),
        end: metadata.end_position(),
        seq_start: ref_seq_start,
        seq_end: ref_seq_end,
        blocks,
      };
      ref_seqs.push(rs);
    }
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
