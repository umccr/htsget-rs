use std::fs::File;
use byteorder::{ByteOrder, LittleEndian, ReadBytesExt};

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

  let path = std::env::current_dir()
    .unwrap()
    .join("data")
    .join("bam")
    .join("htsnexus_test_NA12878.bam.bai");
  let file = File::open(path).unwrap();
  let mut reader = noodles::bam::bai::Reader::new(&file);
  reader.read_header().unwrap();
  let index = reader.read_index().unwrap();
  println!("{:#?}", index);

  let path = std::env::current_dir()
    .unwrap()
    .join("data")
    .join("bam")
    .join("htsnexus_test_NA12878.bam.gzi");
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
