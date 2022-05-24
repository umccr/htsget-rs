use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, BenchmarkGroup, Criterion};
use futures_util::future::join_all;
use htsget_http_core::{JsonResponse, PostRequest, Region};
use htsget_test_utils::server_tests::{default_dir, default_test_config};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::{convert::TryInto, fs, time::Duration};
use std::path::PathBuf;
use tokio::runtime::Runtime;

#[derive(Serialize)]
struct Empty {}

#[derive(Deserialize)]
struct RefserverConfig {
  #[serde(rename = "htsgetConfig")]
  htsget_config: RefserverProps,
}

#[derive(Deserialize)]
struct RefserverProps {
  props: RefserverAddr,
}

#[derive(Deserialize)]
struct RefserverAddr {
  #[serde(rename = "port")]
  _port: u64,
  host: String,
}

const BENCHMARK_DURATION_SECONDS: u64 = 15;
const NUMBER_OF_EXECUTIONS: usize = 150;

async fn request(url: reqwest::Url, json_content: &impl Serialize) -> reqwest::Result<usize> {
  let client = Client::new();
  let response: JsonResponse = client
    .get(url)
    .json(json_content)
    .send()
    .await?
    .json()
    .await?;
  Ok(
    join_all(response.htsget.urls.iter().map(|json_url| async {
      client
        .get(&json_url.url)
        .headers(json_url.headers.borrow().try_into().unwrap())
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap()
        .len()
    }))
    .await
    .into_iter()
    .sum(),
  )
}

fn format_url(url: &str, path: &str) -> reqwest::Url {
  reqwest::Url::parse(url).expect("Invalid url").join(path).expect("Invalid url")
}

fn bench_pair(
  group: &mut BenchmarkGroup<WallTime>,
  name: &str,
  htsget_url: reqwest::Url,
  refserver_url: reqwest::Url,
  json_content: &impl Serialize,
) {
  group.bench_with_input(format!("{} {}", name, "htsget-rs"), &htsget_url, |b, input| {
    b.to_async(Runtime::new().unwrap()).iter(|| request(input.clone(), json_content))
  });
  group.bench_with_input(format!("{} {}", name, "htsget-refserver"), &refserver_url, |b, input| {
    b.to_async(Runtime::new().unwrap()).iter(|| request(input.clone(), json_content))
  });
}

fn start_htsget_rs(config: &Config) {
  
}

fn criterion_benchmark(c: &mut Criterion) {
  let mut group = c.benchmark_group("Requests");
  group
    .sample_size(NUMBER_OF_EXECUTIONS)
    .measurement_time(Duration::from_secs(BENCHMARK_DURATION_SECONDS));

  let config = default_test_config();
  let refserver_config: RefserverConfig =
    serde_json::from_str(&fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches").join("htsget-refserver-config.json")).unwrap()).unwrap();

  let htsget_rs_url = format!("https://{}", config.addr);
  let htsget_refserver_url = refserver_config.htsget_config.props.host;
  bench_pair(
    &mut group,
    "[LIGHT] simple request",
    format_url(
      &htsget_rs_url,
      "reads/bam/htsnexus_test_NA12878",
    ),
    format_url(
      &htsget_refserver_url,
      "reads/htsnexus_test_NA12878",
    ),
    &Empty {},
  );

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
  bench_pair(
    &mut group,
    "[LIGHT] with region",
    format_url(
      &htsget_rs_url,
      "reads/bam/htsnexus_test_NA12878",
    ),
    format_url(
      &htsget_refserver_url,
      "reads/htsnexus_test_NA12878",
    ),
    &json_content,
  );

  let json_content = PostRequest {
    format: None,
    class: None,
    fields: None,
    tags: None,
    notags: None,
    regions: Some(vec![
      Region {
        reference_name: "20".to_string(),
        start: None,
        end: None,
      },
      Region {
        reference_name: "11".to_string(),
        start: Some(4999977),
        end: Some(5008321),
      },
    ]),
  };
  bench_pair(
    &mut group,
    "[LIGHT] with two regions",
    format_url(
      &htsget_rs_url,
      "reads/bam/htsnexus_test_NA12878",
    ),
    format_url(
      &htsget_refserver_url,
      "reads/htsnexus_test_NA12878",
    ),
    &json_content,
  );

  let json_content = PostRequest {
    format: None,
    class: None,
    fields: None,
    tags: None,
    notags: None,
    regions: Some(vec![Region {
      reference_name: "chrM".to_string(),
      start: Some(0),
      end: Some(153),
    }]),
  };
  bench_pair(
    &mut group,
    "[LIGHT] with VCF",
    format_url(
      &htsget_rs_url,
      "variants/vcf/sample1-bcbio-cancer",
    ),
    format_url(
      &htsget_refserver_url,
      "variants/sample1-bcbio-cancer",
    ),
    &json_content,
  );

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
  bench_pair(
    &mut group,
    "[HEAVY] with big VCF",
    format_url(
      &htsget_rs_url,
      "variants/vcf/internationalgenomesample",
    ),
    format_url(
      &htsget_refserver_url,
      "variants/internationalgenomesample",
    ),
    &json_content,
  );

  group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);