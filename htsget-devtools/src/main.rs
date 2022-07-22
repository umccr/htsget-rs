use byteorder::{LittleEndian, ReadBytesExt};
use noodles::csi::index::reference_sequence::bin::Chunk;
use noodles::csi::{BinningIndex, BinningIndexReferenceSequence};
use std::fs::File;

mod bam;
mod bcf;
mod vcf;

fn main() {
  // let path = std::env::current_dir()
  //   .unwrap()
  //   .join("data")
  //   .join("bam")
  //   .join("htsnexus_test_NA12878.bam");
  //
  // let ref_seqs = bam::bam_blocks(path).unwrap();
  // println!("{}", serde_yaml::to_string(&ref_seqs).unwrap());
  //
  // let path = std::env::current_dir()
  //   .unwrap()
  //   .join("data")
  //   .join("vcf")
  //   .join("sample1-bcbio-cancer.vcf.gz");
  //
  // let ref_seqs = vcf::vcf_blocks(path).unwrap();
  // println!("{}", serde_yaml::to_string(&ref_seqs).unwrap());
  //
  // let path = std::env::current_dir()
  //   .unwrap()
  //   .join("data")
  //   .join("bcf")
  //   .join("vcf-spec-v4.3.bcf");
  //
  // let ref_seqs = bcf::bcf_blocks(path).unwrap();
  // println!("{}", serde_yaml::to_string(&ref_seqs).unwrap());

  // let path = std::env::current_dir()
  //   .unwrap()
  //   .join("data")
  //   .join("cram")
  //   .join("htsnexus_test_NA12878.cram.crai");
  // let file = File::open(path).unwrap();
  // let mut reader = noodles::cram::crai::Reader::new(&file);
  // let index = reader.read_index().unwrap();
  // println!("{:#?}", index);

  // let path = std::env::current_dir()
  //   .unwrap()
  //   .join("data")
  //   .join("vcf")
  //   .join("sample1-bcbio-cancer.vcf.gz.tbi");
  // let file = File::open(path).unwrap();
  // let mut reader = noodles::tabix::Reader::new(&file);
  // let index = reader.read_index().unwrap();
  // for ref_seq in index.reference_sequence_names() {
  //   println!("{}", ref_seq);
  // }
  // let mut offsets = index
  //   .reference_sequences()
  //   .iter()
  //   .flat_map(|ref_seq| ref_seq.bins())
  //   .flat_map(|bin| bin.chunks())
  //   .map(|chunk| {
  //     (
  //       chunk.start().compressed() as i32,
  //       chunk.end().compressed() as i32,
  //     )
  //   })
  //   .collect::<Vec<(i32, i32)>>();
  // for lin in index.reference_sequences() {
  //   for inter in lin.intervals() {
  //     offsets.push((inter.compressed() as i32, -1));
  //   }
  //   if let Some(meta) = lin.metadata() {
  //     offsets.push((
  //       meta.start_position().compressed() as i32,
  //       meta.end_position().compressed() as i32,
  //     ))
  //   }
  // }
  //
  // println!("{:#?}", offsets);
  //
  // let path = std::env::current_dir()
  //   .unwrap()
  //   .join("data")
  //   .join("bcf")
  //   .join("sample1-bcbio-cancer.bcf.csi");
  // let file = File::open(path).unwrap();
  // let mut reader = noodles::csi::Reader::new(&file);
  // let index = reader.read_index().unwrap();
  // let mut offsets = index
  //   .reference_sequences()
  //   .iter()
  //   .flat_map(|ref_seq| ref_seq.bins())
  //   .flat_map(|bin| bin.chunks())
  //   .map(|chunk| {
  //     (
  //       chunk.start().compressed() as i32,
  //       chunk.end().compressed() as i32,
  //     )
  //   })
  //   .collect::<Vec<(i32, i32)>>();
  // for lin in index.reference_sequences() {
  //   if let Some(meta) = lin.metadata() {
  //     offsets.push((
  //       meta.start_position().compressed() as i32,
  //       meta.end_position().compressed() as i32,
  //     ))
  //   }
  // }
  // println!("{:#?}", offsets);

  let path = std::env::current_dir()
    .unwrap()
    .join("data")
    .join("bam")
    .join("htsnexus_test_NA12878.bam.bai");
  let file = File::open(path).unwrap();
  let mut reader = noodles::bam::bai::Reader::new(&file);
  reader.read_header().unwrap();
  let index = reader.read_index().unwrap();
  let mut offsets: Vec<(i64, i64)> = Vec::new();
  for ref_seq in index.reference_sequences() {
    for bin in ref_seq.bins() {
      for chunk in bin.chunks() {
        offsets.push((chunk.start().compressed() as i64, chunk.end().compressed() as i64));
      }
    }
    for lin in ref_seq.intervals() {
      offsets.push((lin.compressed() as i64, -1))
    }
    if let Some(metadata) = ref_seq.metadata() {
      offsets.push((metadata.start_position().compressed() as i64, metadata.end_position().compressed() as i64))
    }
  }
  println!("{:#?}", offsets);
  // let path = std::env::current_dir()
  //   .unwrap()
  //   .join("data")
  //   .join("bam")
  //   .join("htsnexus_test_NA12878.bam.bai");
  // let file = File::open(path).unwrap();
  // let mut reader = noodles::bam::bai::Reader::new(&file);
  // reader.read_header().unwrap();
  // let index = reader.read_index().unwrap();
  // // println!("{:#?}", index);
  //
  // let mut chunks: Vec<u64> = Vec::new();
  // for ref_seq in index.reference_sequences() {
  //   for bin in ref_seq.bins() {
  //     for chunk in bin.chunks() {
  //       chunks.push(chunk.start().compressed());
  //       chunks.push(chunk.end().compressed());
  //     }
  //   }
  //   for lin in ref_seq.intervals() {
  //     chunks.push(lin.compressed());
  //   }
  //   if let Some(metadata) = ref_seq.metadata() {
  //     chunks.push(metadata.end_position().compressed());
  //     chunks.push(metadata.start_position().compressed());
  //   }
  // }
  //
  // chunks.sort();
  // chunks.dedup();
  // println!("{:#?}", chunks);
  //
  // let intervals = index
  //   .reference_sequences()
  //   .iter()
  //   .rev()
  //   .find_map(|rs| rs.intervals().last().cloned());
  // println!("{:?}", intervals);

  let path = std::env::current_dir()
    .unwrap()
    .join("data")
    .join("vcf")
    .join("sample1-bcbio-cancer.vcf.gz.gzi");
  let mut file = File::open(path).unwrap();
  let mut values: Vec<u64> = Vec::new();
  while let Ok(value) = file.read_u64::<LittleEndian>() {
    values.push(value);
  }
  println!("Number of entries: {:#?}", values.first());
  // Get every second value, which is the compressed offset, pointing to the start of a BGZF block.
  let values = values.iter().skip(1).step_by(2).collect::<Vec<_>>();
  println!("{:#?}", values);
}
