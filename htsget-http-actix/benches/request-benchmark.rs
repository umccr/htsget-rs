use criterion::{criterion_group, criterion_main, Criterion};
use htsget_http_core::JsonResponse;
use reqwest::{blocking::Client, IntoUrl};
use serde::Serialize;
use std::collections::HashMap;
use std::{convert::TryInto, time::Duration};
#[derive(Serialize)]
struct Empty {}

const HTSGET_RS_URL: &str = "http://localhost:8080/reads/data/bam/htsnexus_test_NA12878";
const HTSGET_REFSERVER_URL: &str = "http://localhost:8081/reads/htsnexus_test_NA12878";

fn request(url: impl IntoUrl) -> usize {
  let client = Client::new();
  let response: JsonResponse = client
    .get(url)
    .json(&Empty {})
    .send()
    .unwrap()
    .json()
    .unwrap();
  response
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
    .sum()
}

fn criterion_benchmark(c: &mut Criterion) {
  let mut group = c.benchmark_group("Requests");
  group
    .sample_size(500)
    .measurement_time(Duration::from_secs(10));
  group.bench_function("htsget-rs", |b| b.iter(|| request(HTSGET_RS_URL)));
  println!("Download size: {} bytes", request(HTSGET_RS_URL));
  group.bench_function("htsget-refserver", |b| {
    b.iter(|| request(HTSGET_REFSERVER_URL))
  });
  println!("Download size: {} bytes", request(HTSGET_REFSERVER_URL));
  group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
