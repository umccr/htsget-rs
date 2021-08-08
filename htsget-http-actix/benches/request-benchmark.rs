use criterion::{criterion_group, criterion_main, Criterion};
use htsget_http_core::{JsonResponse, PostRequest, Region};
use reqwest::{blocking::Client, Error as ActixError, IntoUrl};
use serde::Serialize;
use std::collections::HashMap;
use std::{convert::TryInto, time::Duration};
#[derive(Serialize)]
struct Empty {}

const HTSGET_RS_URL: &str = "http://localhost:8080/reads/data/bam/htsnexus_test_NA12878";
const HTSGET_REFSERVER_URL: &str = "http://localhost:8081/reads/htsnexus_test_NA12878";

fn request(url: impl IntoUrl, json_content: &impl Serialize) -> Result<usize, ActixError> {
  let client = Client::new();
  let response: JsonResponse = client.get(url).json(json_content).send()?.json()?;
  Ok(
    response
      .htsget
      .urls
      .iter()
      .map(|json_url| {
        Ok(
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
            .send()?
            .text()?
            .len(),
        )
      })
      .collect::<Result<Vec<_>, ActixError>>()?
      .into_iter()
      .sum(),
  )
}

fn criterion_benchmark(c: &mut Criterion) {
  let mut group = c.benchmark_group("Requests");
  group
    .sample_size(500)
    .measurement_time(Duration::from_secs(10));

  println!(
    "Download size: {} bytes",
    request(HTSGET_RS_URL, &Empty {}).expect("Error during the request")
  );
  group.bench_function("htsget-rs simple request", |b| {
    b.iter(|| request(HTSGET_RS_URL, &Empty {}))
  });
  println!(
    "Download size: {} bytes",
    request(HTSGET_REFSERVER_URL, &Empty {}).expect("Error during the request")
  );
  group.bench_function("htsget-refserver simple request", |b| {
    b.iter(|| request(HTSGET_REFSERVER_URL, &Empty {}))
  });

  let json_content = PostRequest {
    format: None,
    class: None,
    fields: None,
    tags: None,
    notags: None,
    regions: Some(vec![Region {
      reference_name: "20".to_string(),
      start: None,
      end: None,
    }]),
  };
  println!(
    "Download size: {} bytes",
    request(HTSGET_RS_URL, &json_content).expect("Error during the request")
  );
  group.bench_function("htsget-rs with region", |b| {
    b.iter(|| request(HTSGET_RS_URL, &json_content))
  });
  println!(
    "Download size: {} bytes",
    request(HTSGET_REFSERVER_URL, &json_content).expect("Error during the request")
  );
  group.bench_function("htsget-refserver with region", |b| {
    b.iter(|| request(HTSGET_REFSERVER_URL, &json_content))
  });

  group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
