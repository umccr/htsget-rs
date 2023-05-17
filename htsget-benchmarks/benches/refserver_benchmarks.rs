// use std::process::{Child, Command};
// use serde::{Deserialize, Serialize};
// use std::path::PathBuf;
// use std::time::Duration;
// use std::thread::sleep;
// use std::fs;

// use htsget_test::http_tests::default_dir;
// use reqwest::blocking::Client;

// use criterion::{criterion_group, criterion_main, BenchmarkGroup, Criterion};

// const REFSERVER_DOCKER_IMAGE: &str = "ga4gh/htsget-refserver:1.5.0";

// #[derive(Serialize)]
// struct Empty;

// #[derive(Deserialize)]
// struct RefserverConfig {
//   #[serde(rename = "htsgetConfig")]
//   htsget_config: RefserverProps,
// }

// #[derive(Deserialize)]
// struct RefserverProps {
//   props: RefserverAddr,
// }

// #[derive(Deserialize)]
// struct RefserverAddr {
//   port: u64,
//   host: String,
// }
// struct DropGuard(Child);

// impl Drop for DropGuard {
//   fn drop(&mut self) {
//     drop(self.0.kill());
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

// fn format_url(url: &str, path: &str) -> reqwest::Url {
//   reqwest::Url::parse(url)
//     .expect("invalid url")
//     .join(path)
//     .expect("invalid url")
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

// fn start_htsget_refserver() -> (DropGuard, String) {
//     let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
//       .join("benches")
//       .join("htsget-refserver-config.json");
//     let refserver_config: RefserverConfig =
//       serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
  
//     new_command("docker")
//       .arg("image")
//       .arg("pull")
//       .arg(REFSERVER_DOCKER_IMAGE)
//       .spawn()
//       .unwrap()
//       .wait()
//       .unwrap();
  
//     let child = new_command("docker")
//       .current_dir(default_dir())
//       .arg("container")
//       .arg("run")
//       .arg("-p")
//       .arg(format!(
//         "{}:3000",
//         &refserver_config.htsget_config.props.port
//       ))
//       .arg("-v")
//       .arg(format!(
//         "{}:/data",
//         default_dir()
//           .join("data")
//           .canonicalize()
//           .unwrap()
//           .to_string_lossy()
//       ))
//       .arg("-v")
//       .arg(format!(
//         "{}:/config",
//         &config_path
//           .canonicalize()
//           .unwrap()
//           .parent()
//           .unwrap()
//           .to_string_lossy()
//       ))
//       .arg(REFSERVER_DOCKER_IMAGE)
//       .arg("./htsget-refserver")
//       .arg("-config")
//       .arg("/config/htsget-refserver-config.json")
//       .spawn()
//       .unwrap();
  
//     let refserver_url = refserver_config.htsget_config.props.host;
//     query_server_until_response(&format_url(&refserver_url, "reads/service-info"));
  
//     (DropGuard(child), refserver_url)
//   }


//   fn criterion_benchmark(_c: &mut Criterion) {
//     let (_htsget_refserver_server, htsget_refserver_url) = start_htsget_refserver();
//     //format_url(&htsget_refserver_url, "reads/htsnexus_test_NA12878"),
//     todo!()
//   }