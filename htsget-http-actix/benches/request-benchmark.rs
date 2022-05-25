use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, BenchmarkGroup, Criterion};
use htsget_http_core::{JsonResponse, PostRequest, Region};
use htsget_test_utils::server_tests::{default_dir, default_test_config};
use htsget_test_utils::util::generate_test_certificates;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::thread::sleep;
use std::{convert::TryInto, fs, time::Duration};
use tempfile::TempDir;

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
  port: u64,
  host: String,
}

const REFSERVER_DOCKER_IMAGE: &str = "ga4gh/htsget-refserver:1.5.0";
const BENCHMARK_DURATION_SECONDS: u64 = 15;
const NUMBER_OF_EXECUTIONS: usize = 150;

fn request(url: reqwest::Url, json_content: &impl Serialize) -> reqwest::Result<usize> {
  let client = Client::new();
  let response: JsonResponse = client.get(url).json(json_content).send()?.json()?;
  Ok(
    response
      .htsget
      .urls
      .iter()
      .map(|json_url| {
        client
          .get(&json_url.url)
          .headers(json_url.headers.borrow().try_into().unwrap())
          .send()
          .unwrap()
          .bytes()
          .unwrap()
          .len()
      })
      .sum(),
  )
}

fn format_url(url: &str, path: &str) -> reqwest::Url {
  reqwest::Url::parse(url)
    .expect("Invalid url")
    .join(path)
    .expect("Invalid url")
}

fn bench_pair(
  group: &mut BenchmarkGroup<WallTime>,
  name: &str,
  htsget_url: reqwest::Url,
  refserver_url: reqwest::Url,
  json_content: &impl Serialize,
) {
  group.bench_with_input(
    format!("{} {}", name, "htsget-rs"),
    &htsget_url,
    |b, input| b.iter(|| request(input.clone(), json_content)),
  );
  group.bench_with_input(
    format!("{} {}", name, "htsget-refserver"),
    &refserver_url,
    |b, input| b.iter(|| request(input.clone(), json_content)),
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

fn query_server_until_response(url: reqwest::Url) {
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

fn start_htsget_rs() -> (Child, String) {
  let config = default_test_config();

  let base_path = TempDir::new().unwrap();
  let (key_path, cert_path) = generate_test_certificates(base_path.path(), "key.pem", "cert.pem");

  let child = new_command("cargo")
    .current_dir(default_dir())
    .arg("run")
    .arg("-p")
    .arg("htsget-http-actix")
    .env("HTSGET_TICKET_SERVER_KEY", key_path)
    .env("HTSGET_TICKET_SERVER_CERT", cert_path)
    .spawn()
    .unwrap();

  let htsget_rs_url = format!("http://{}", config.addr);
  query_server_until_response(format_url(&htsget_rs_url, "reads/service-info"));

  (child, htsget_rs_url)
}

fn start_htsget_refserver() -> (Child, String) {
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
    .arg("-d")
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
  query_server_until_response(format_url(&refserver_url, "reads/service-info"));

  (child, refserver_url)
}

fn criterion_benchmark(c: &mut Criterion) {
  let mut group = c.benchmark_group("Requests");
  group
    .sample_size(NUMBER_OF_EXECUTIONS)
    .measurement_time(Duration::from_secs(BENCHMARK_DURATION_SECONDS));

  let (mut htsget_rs_server, htsget_rs_url) = start_htsget_rs();
  let (mut htsget_refserver_server, htsget_refserver_url) = start_htsget_refserver();

  bench_pair(
    &mut group,
    "[LIGHT] simple request",
    format_url(&htsget_rs_url, "reads/bam/htsnexus_test_NA12878"),
    format_url(&htsget_refserver_url, "reads/htsnexus_test_NA12878"),
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
      start: Some(0),
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

  htsget_rs_server.kill().unwrap();
  htsget_refserver_server.kill().unwrap();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
