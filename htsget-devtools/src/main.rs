mod bam;
mod vcf;

fn main() {
  let path = std::env::current_dir()
    .unwrap()
    .join("data")
    .join("bam")
    .join("htsnexus_test_NA12878.bam");

  let ref_seqs = bam::bam_blocks(path).unwrap();
  println!("{}", serde_yaml::to_string(&ref_seqs).unwrap());

  let path = std::env::current_dir()
    .unwrap()
    .join("data")
    .join("vcf")
    .join("sample1-bcbio-cancer.vcf.gz");

  let ref_seqs = vcf::vcf_blocks(path).unwrap();
  println!("{}", serde_yaml::to_string(&ref_seqs).unwrap());
}
