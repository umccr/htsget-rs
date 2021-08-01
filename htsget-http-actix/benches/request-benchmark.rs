use criterion::{criterion_group, criterion_main, Criterion};
use reqwest::{blocking::Client, Url};

fn request() {
  let mut url = Url::parse("http://127.0.0.1/variants/data/vcf/sample1-bcbio-cancer").unwrap();
  url.set_port(Some(8080)).unwrap();
  Client::new().post(url).send().unwrap();
}

fn criterion_benchmark(c: &mut Criterion) {
  c.bench_function("simple POST request ", |b| b.iter(|| request()));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
