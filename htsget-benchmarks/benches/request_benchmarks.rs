// use std::process::{Child, Command};
// use std::thread::sleep;
// use std::{convert::TryInto, time::Duration};

// use criterion::measurement::WallTime;
// use criterion::{criterion_group, criterion_main, BenchmarkGroup, Criterion};
// use reqwest::blocking::Client;
// use serde::{Serialize};

// use htsget_config::types::{Headers, JsonResponse};
// use htsget_http::{PostRequest, Region};
// use htsget_test::http_tests::{default_config_fixed_port, default_dir, default_dir_data};

// const BENCHMARK_DURATION_SECONDS: u64 = 30;
// const NUMBER_OF_SAMPLES: usize = 50;

// struct DropGuard(Child);

// impl Drop for DropGuard {
//   fn drop(&mut self) {
//     drop(self.0.kill());
//   }
// }

// fn request(url: reqwest::Url, json_content: &impl Serialize, client: &Client) -> usize {
//   let response: JsonResponse = client
//     .post(url)
//     .json(json_content)
//     .send()
//     .unwrap()
//     .json()
//     .unwrap();

//   response
//     .htsget
//     .urls
//     .iter()
//     .map(|json_url| {
//       Ok(
//         client
//           .get(&json_url.url)
//           .headers(
//             json_url
//               .headers
//               .as_ref()
//               .unwrap_or(&Headers::default())
//               .as_ref_inner()
//               .try_into()
//               .unwrap(),
//           )
//           .send()?
//           .bytes()?
//           .len(),
//       )
//     })
//     .fold(0, |acc, x: Result<usize>| acc + x.unwrap_or(0))
// }

// fn format_url(url: &str, path: &str) -> reqwest::Url {
//   reqwest::Url::parse(url)
//     .expect("invalid url")
//     .join(path)
//     .expect("invalid url")
// }

// fn bench_query(group: &mut BenchmarkGroup<WallTime>, name: &str, query: Query) {
//   group.bench_with_input(name, &query, |b, input| {
//     b.to_async(Runtime::new().unwrap())
//       .iter(|| perform_query(input.clone()))
//   });
// }

// fn query_server_until_response(url: &reqwest::Url) {
//   let client = Client::new();
//   for _ in 0..120 {
//     sleep(Duration::from_secs(1));
//     if let Err(err) = client.get(url.clone()).send() {
//       if err.is_connect() {
//         continue;
//       }
//     }
//     break;
//   }
// }

// #[cfg(target_os = "windows")]
// pub fn new_command(cmd: &str) -> Command {
//   let mut command = Command::new("cmd.exe");
//   command.arg("/c");
//   command.arg(cmd);
//   command
// }

// #[cfg(not(target_os = "windows"))]
// pub fn new_command(cmd: &str) -> Command {
//   Command::new(cmd)
// }

// fn start_htsget_rs() -> (DropGuard, String) {
//   let config = default_config_fixed_port();

//   let child = new_command("cargo")
//     .current_dir(default_dir())
//     .arg("run")
//     .arg("-p")
//     .arg("htsget-actix")
//     .arg("--no-default-features")
//     .env("HTSGET_PATH", default_dir_data())
//     .env("RUST_LOG", "warn")
//     .spawn()
//     .unwrap();

//   let htsget_rs_url = format!("http://{}", config.ticket_server().addr());
//   query_server_until_response(&format_url(&htsget_rs_url, "reads/service-info"));
//   let htsget_rs_ticket_url = format!("http://{}", config.data_server().addr());
//   query_server_until_response(&format_url(&htsget_rs_ticket_url, ""));

//   (DropGuard(child), htsget_rs_url)
// }


// fn criterion_benchmark(c: &mut Criterion) {
//   let mut group = c.benchmark_group("Requests");
//   group
//     .sample_size(NUMBER_OF_SAMPLES)
//     .measurement_time(Duration::from_secs(BENCHMARK_DURATION_SECONDS));

//   let (_htsget_rs_server, htsget_rs_url) = start_htsget_rs();

//   bench_query(
//     &mut group,
//     "[LIGHT] simple request",
//     format_url(&htsget_rs_url, "reads/data/bam/htsnexus_test_NA12878"),
//   );

//   bench_query(
//     &mut group,
//     "[LIGHT] with region",
//     format_url(&htsget_rs_url, "reads/data/bam/htsnexus_test_NA12878"),
//   );

//   bench_query(
//     &mut group,
//     "[LIGHT] with two regions",
//     format_url(&htsget_rs_url, "reads/data/bam/htsnexus_test_NA12878"),
//   );

//   bench_query(
//     &mut group,
//     "[LIGHT] with VCF",
//     format_url(&htsget_rs_url, "variants/data/vcf/sample1-bcbio-cancer"),
//   );

//   // bench_pair(
//   //   &mut group,
//   //   "[HEAVY] with big VCF",
//   //   format_url(
//   //     &htsget_rs_url,
//   //     "variants/data/vcf/internationalgenomesample",
//   //   ),
//   //   format_url(&htsget_refserver_url, "variants/internationalgenomesample"),
//   //   &json_content,
//   // );

//   group.finish();
// }

// criterion_group!(benches, criterion_benchmark);
// criterion_main!(benches);
