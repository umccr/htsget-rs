use criterion::{criterion_group, criterion_main, Criterion};
use htsget_http_core::JsonResponse;
use reqwest::{blocking::Client, Url};
use serde::Serialize;
use std::collections::HashMap;
use std::{convert::TryInto, time::Duration};
#[derive(Serialize)]
struct Empty {}

fn request(url: Url) {
  let client = Client::new();
  let response: JsonResponse = client
    .get(url)
    .json(&Empty {})
    .send()
    .unwrap()
    .json()
    .unwrap();
  let download_size: usize = response
    .htsget
    .urls
    .iter()
    .map(|json_url| {
      client
        .get(&json_url.url)
        .headers(
          json_url
            .headers
            .as_ref()
            .unwrap_or(&HashMap::new())
            .try_into()
            .unwrap(),
        )
        .send()
        .unwrap()
        .text()
        .unwrap()
        .len()
    })
    .sum();
  println!("{}", download_size)
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
