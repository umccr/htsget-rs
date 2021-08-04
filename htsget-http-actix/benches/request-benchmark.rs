use criterion::{criterion_group, criterion_main, Criterion};
use htsget_http_core::JsonResponse;
use reqwest::{blocking::Client, Url};
use std::time::Duration;

fn request(url: Url) {
  let _: JsonResponse = Client::new().get(url).send().unwrap().json().unwrap();
}

fn criterion_benchmark(c: &mut Criterion) {
  let mut group = c.benchmark_group("Requests");
  group
    .sample_size(500)
    .measurement_time(Duration::from_secs(10));
  group.bench_function("htsget-rs", |b| {
    b.iter(|| {
      let mut url = Url::parse("http://localhost/reads/data/bam/htsnexus_test_NA12878").unwrap();
      url.set_port(Some(8080)).unwrap();
      request(url)
    })
  });
  group.bench_function("htsget-refserver", |b| {
    b.iter(|| {
      let mut url = Url::parse("http://localhost/reads/htsnexus_test_NA12878").unwrap();
      url.set_port(Some(8081)).unwrap();
      request(url)
    })
  });
  group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
