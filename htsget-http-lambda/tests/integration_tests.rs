//! Integration tests for htsget-http-lambda using cargo lambda.
//!

use std::process::Stdio;
use std::sync::Once;
use std::thread::sleep;
use std::time::Duration;

use tokio::process::Command;

use htsget_search::htsget::Class;
use htsget_test_utils::server_tests::{
  default_test_config, get_test_file, test_response, test_response_service_info,
};

static INIT_CARGO_LAMBDA_WATCH: Once = Once::new();

fn cargo_lambda_watch() {
  INIT_CARGO_LAMBDA_WATCH.call_once(|| {
    std::env::set_current_dir(std::env::current_dir().unwrap().parent().unwrap()).unwrap();
    Command::new("cargo")
      .args(["lambda", "watch"])
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .expect("Failed to start cargo lambda watch.");
  });
}

async fn cargo_lambda_invoke(data: &str) -> String {
  let mut invoke = Command::new("cargo");
  invoke
    .args([
      "lambda",
      "invoke",
      "htsget-http-lambda",
      "--data-ascii",
      data,
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null());
  for _ in 0..20 {
    sleep(Duration::from_secs(1));
    if invoke.status().await.unwrap().success() {
      return String::from_utf8(
        invoke
          .output()
          .await
          .expect("Failed to execute cargo lambda invoke.")
          .stdout,
      )
      .expect("Expected valid output.");
    }
  }
  panic!("Failed to invoke request.");
}

async fn execute_cargo_lambda(data: &str) -> String {
  cargo_lambda_watch();
  cargo_lambda_invoke(data).await
}

#[tokio::test]
async fn test_get() {
  let event = get_test_file("data/events/event_get.json");
  let response = execute_cargo_lambda(&event).await;
  test_response(
    &serde_json::from_slice(response.as_bytes()).unwrap(),
    &default_test_config(),
    Class::Body,
  );
}

#[tokio::test]
async fn test_post() {
  let event = get_test_file("data/events/event_post.json");
  let response = execute_cargo_lambda(&event).await;
  test_response(
    &serde_json::from_slice(response.as_bytes()).unwrap(),
    &default_test_config(),
    Class::Body,
  );
}

#[tokio::test]
async fn test_parameterized_get() {
  let event = get_test_file("data/events/event_parameterized_get.json");
  let response = execute_cargo_lambda(&event).await;
  test_response(
    &serde_json::from_slice(response.as_bytes()).unwrap(),
    &default_test_config(),
    Class::Header,
  );
}

#[tokio::test]
async fn test_parameterized_post() {
  let event = get_test_file("data/events/event_parameterized_post.json");
  let response = execute_cargo_lambda(&event).await;
  test_response(
    &serde_json::from_slice(response.as_bytes()).unwrap(),
    &default_test_config(),
    Class::Body,
  );
}

#[tokio::test]
async fn test_parameterized_post_class_header() {
  let event = get_test_file("data/events/event_parameterized_post_class_header.json");
  let response = execute_cargo_lambda(&event).await;
  println!("{:?}", response);
  test_response(
    &serde_json::from_slice(response.as_bytes()).unwrap(),
    &default_test_config(),
    Class::Header,
  );
}

#[tokio::test]
async fn test_service_info() {
  let event = get_test_file("data/events/event_service_info.json");
  let response = execute_cargo_lambda(&event).await;
  test_response_service_info(&serde_json::from_slice(response.as_bytes()).unwrap());
}
