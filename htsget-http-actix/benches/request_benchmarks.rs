use std::path::PathBuf;
use std::process::{Child, Command};
use std::thread::sleep;
use std::{convert::TryInto, fs, time::Duration};

use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, BenchmarkGroup, Criterion};
use reqwest::blocking::Client;
use reqwest::blocking::ClientBuilder;
use reqwest::Result;
use serde::{Deserialize, Serialize};

use htsget_http_core::{PostRequest, Region};
use htsget_search::htsget::{Headers, JsonResponse};
use htsget_test_utils::http_tests::{default_config_fixed_port, default_dir, default_dir_data};

const REFSERVER_DOCKER_IMAGE: &str = "ga4gh/htsget-refserver:1.5.0";
const BENCHMARK_DURATION_SECONDS: u64 = 30;
const NUMBER_OF_SAMPLES: usize = 100;

#[derive(Serialize)]
struct Empty;

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
  port: u64,
  host: String,
}

struct DropGuard(Child);

impl Drop for DropGuard {
  fn drop(&mut self) {
    drop(self.0.kill());
  }
}

fn request(url: reqwest::Url, json_content: &impl Serialize, client: &Client) -> usize {
  let response: JsonResponse = client
    .post(url)
    .json(json_content)
    .send()
    .unwrap()
    .json()
    .unwrap();
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
              .unwrap_or(&Headers::default())
              .as_ref_inner()
              .try_into()
              .unwrap(),
          )
          .send()?
          .bytes()?
          .len(),
      )
    })
    .fold(0, |acc, x: Result<usize>| acc + x.unwrap_or(0))
}

fn format_url(url: &str, path: &str) -> reqwest::Url {
  reqwest::Url::parse(url)
    .expect("invalid url")
    .join(path)
    .expect("invalid url")
}

fn bench_pair(
  group: &mut BenchmarkGroup<WallTime>,
  name: &str,
  htsget_url: reqwest::Url,
  refserver_url: reqwest::Url,
  json_content: &impl Serialize,
) {
  let client = ClientBuilder::new()
    .danger_accept_invalid_certs(true)
    .use_rustls_tls()
    .build()
    .unwrap();
  group.bench_with_input(
    format!("{} {}", name, "htsget-rs"),
    &htsget_url,
    |b, input| b.iter(|| request(input.clone(), json_content, &client)),
  );
  group.bench_with_input(
    format!("{} {}", name, "htsget-refserver"),
    &refserver_url,
    |b, input| b.iter(|| request(input.clone(), json_content, &client)),
  );
}

#[cfg(target_os = "windows")]
pub fn new_command(cmd: &str) -> Command {
  let mut command = Command::new("cmd.exe");
  command.arg("/c");
  command.arg(cmd);
  command
}

#[cfg(not(target_os = "windows"))]
pub fn new_command(cmd: &str) -> Command {
  Command::new(cmd)
}

fn query_server_until_response(url: &reqwest::Url) {
  let client = Client::new();
  for _ in 0..120 {
    sleep(Duration::from_secs(1));
    if let Err(err) = client.get(url.clone()).send() {
      if err.is_connect() {
        continue;
      }
    }
    break;
  }
}

fn start_htsget_rs() -> (DropGuard, String) {
  let config = default_config_fixed_port();

  let child = new_command("cargo")
    .current_dir(default_dir())
    .arg("run")
    .arg("-p")
    .arg("htsget-http-actix")
    .env("HTSGET_PATH", default_dir_data())
    .env("RUST_LOG", "warn")
    .spawn()
    .unwrap();

  let htsget_rs_url = format!("http://{}", config.ticket_server_config.ticket_server_addr);
  query_server_until_response(&format_url(&htsget_rs_url, "reads/service-info"));
  let htsget_rs_ticket_url = format!("http://{}", config.data_server_config.data_server_addr);
  query_server_until_response(&format_url(&htsget_rs_ticket_url, ""));

  (DropGuard(child), htsget_rs_url)
}

fn start_htsget_refserver() -> (DropGuard, String) {
  let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("benches")
    .join("htsget-refserver-config.json");
  let refserver_config: RefserverConfig =
    serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();

  new_command("docker")
    .arg("image")
    .arg("pull")
    .arg(REFSERVER_DOCKER_IMAGE)
    .spawn()
    .unwrap()
    .wait()
    .unwrap();

  let child = new_command("docker")
    .current_dir(default_dir())
    .arg("container")
    .arg("run")
    .arg("-p")
    .arg(format!(
      "{}:3000",
      &refserver_config.htsget_config.props.port
    ))
    .arg("-v")
    .arg(format!(
      "{}:/data",
      default_dir()
        .join("data")
        .canonicalize()
        .unwrap()
        .to_string_lossy()
    ))
    .arg("-v")
    .arg(format!(
      "{}:/config",
      &config_path
        .canonicalize()
        .unwrap()
        .parent()
        .unwrap()
        .to_string_lossy()
    ))
    .arg(REFSERVER_DOCKER_IMAGE)
    .arg("./htsget-refserver")
    .arg("-config")
    .arg("/config/htsget-refserver-config.json")
    .spawn()
    .unwrap();

  let refserver_url = refserver_config.htsget_config.props.host;
  query_server_until_response(&format_url(&refserver_url, "reads/service-info"));

  (DropGuard(child), refserver_url)
}

fn criterion_benchmark(c: &mut Criterion) {
  let mut group = c.benchmark_group("Requests");
  group
    .sample_size(NUMBER_OF_SAMPLES)
    .measurement_time(Duration::from_secs(BENCHMARK_DURATION_SECONDS));

  let (_htsget_rs_server, htsget_rs_url) = start_htsget_rs();
  let (_htsget_refserver_server, htsget_refserver_url) = start_htsget_refserver();

  let json_content = PostRequest {
    format: None,
    class: None,
    fields: None,
    tags: None,
    notags: None,
    regions: None,
  };
  bench_pair(
    &mut group,
    "[LIGHT] simple request",
    format_url(&htsget_rs_url, "reads/bam/htsnexus_test_NA12878"),
    format_url(&htsget_refserver_url, "reads/htsnexus_test_NA12878"),
    &json_content,
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
    format_url(&htsget_rs_url, "reads/bam/htsnexus_test_NA12878"),
    format_url(&htsget_refserver_url, "reads/htsnexus_test_NA12878"),
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
    format_url(&htsget_rs_url, "reads/bam/htsnexus_test_NA12878"),
    format_url(&htsget_refserver_url, "reads/htsnexus_test_NA12878"),
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
      start: Some(1),
      end: Some(153),
    }]),
  };
  bench_pair(
    &mut group,
    "[LIGHT] with VCF",
    format_url(&htsget_rs_url, "variants/vcf/sample1-bcbio-cancer"),
    format_url(&htsget_refserver_url, "variants/sample1-bcbio-cancer"),
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
    format_url(&htsget_rs_url, "variants/vcf/internationalgenomesample"),
    format_url(&htsget_refserver_url, "variants/internationalgenomesample"),
    &json_content,
  );

  group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
