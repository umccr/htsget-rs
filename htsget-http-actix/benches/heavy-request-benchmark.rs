use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, BenchmarkGroup, Criterion};
use htsget_http_core::{JsonResponse, PostRequest, Region};
use reqwest::{blocking::Client, Error as ActixError};
use serde::Serialize;
use std::collections::HashMap;
use std::{convert::TryInto, time::Duration};

#[derive(Serialize)]
struct Empty {}

const HTSGET_RS_VCF_URL: &str = "http://localhost:8080/variants/data/vcf/internationalgenomesample";
const HTSGET_REFSERVER_VCF_URL: &str = "http://localhost:8081/variants/internationalgenomesample";

fn request(url: &str, json_content: &impl Serialize) -> Result<usize, ActixError> {
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
            .bytes()?
            .len(),
        )
      })
      .collect::<Result<Vec<_>, ActixError>>()?
      .into_iter()
      .sum(),
  )
}

fn bench_request(
  group: &mut BenchmarkGroup<WallTime>,
  name: &str,
  url: &str,
  json_content: &impl Serialize,
) {
  println!(
    "\n\nDownload size: {} bytes",
    request(url, json_content).expect("Error during the request")
  );
  group.bench_function(name, |b| b.iter(|| request(url, json_content)));
}

fn criterion_benchmark(c: &mut Criterion) {
  let mut group = c.benchmark_group("Requests");
  group
    .sample_size(150)
    .measurement_time(Duration::from_secs(15));

  let json_content = PostRequest {
    format: None,
    class: None,
    fields: None,
    tags: None,
    notags: None,
    regions: Some(vec![Region {
      reference_name: "14".to_string(),
      start: None,
      end: None,
    }]),
  };
  bench_request(
    &mut group,
    "htsget-rs big VCF file",
    HTSGET_RS_VCF_URL,
    &json_content,
  );
  bench_request(
    &mut group,
    "htsget-refserver big VCF file",
    HTSGET_REFSERVER_VCF_URL,
    &json_content,
  );

  group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
