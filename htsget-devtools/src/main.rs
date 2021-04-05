mod bam;

fn main() {
  let path = std::env::current_dir()
    .unwrap()
    .join("data")
    .join("htsnexus_test_NA12878.bam");

  let ref_seqs = bam::bam_blocks(path).unwrap();
  println!("{}", serde_yaml::to_string(&ref_seqs).unwrap());
}
