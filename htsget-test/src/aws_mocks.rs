use std::fs::{metadata, read};
use std::future::Future;
use std::path::{Path, PathBuf};

use aws_sdk_s3::Client;
use aws_sdk_s3::config::{BehaviorVersion, Region};
use aws_sdk_s3::operation::get_object::{GetObjectError, GetObjectOutput};
use aws_sdk_s3::operation::head_object::{HeadObjectError, HeadObjectOutput};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::error::{NoSuchKey, NotFound};
use aws_smithy_mocks::{RuleMode, mock, mock_client};
use tempfile::TempDir;

/// Default domain to use for mock s3 server.
pub const DEFAULT_DOMAIN_NAME: &str = "localhost:0";

/// Default region to use for mock s3 server.
pub const DEFAULT_REGION: &str = "ap-southeast-2";

/// Resolve the path for an S3 object.
fn object_path(base: &Path, bucket: Option<&str>, key: Option<&str>) -> PathBuf {
  base
    .join(bucket.unwrap_or_default())
    .join(key.unwrap_or_default())
}

/// Slice `data` according to an S3 `Range` header.
fn slice_range(data: &[u8], range: Option<&str>) -> Vec<u8> {
  let Some((start, end)) = range
    .and_then(|range| range.strip_prefix("bytes="))
    .and_then(|range| range.split_once('-'))
  else {
    return data.to_vec();
  };

  let start = start.parse().unwrap_or(0);
  let end = end
    .parse::<usize>()
    .map_or(data.len(), |end| end + 1)
    .min(data.len());

  data.get(start..end).unwrap_or_default().to_vec()
}

/// Build a mock `aws_sdk_s3` client that serves objects from `base_path`.
fn mock_s3_client(base_path: &Path, domain_name: &str, region: &'static str) -> Client {
  let base = base_path.to_path_buf();
  let matches = base.clone();
  let get_ok = mock!(Client::get_object)
    .match_requests(move |req| object_path(&matches, req.bucket(), req.key()).is_file())
    .then_compute_output(move |req| {
      let data = read(object_path(&base, req.bucket(), req.key())).unwrap_or_default();
      let body = slice_range(&data, req.range());
      GetObjectOutput::builder()
        .content_length(body.len() as i64)
        .body(ByteStream::from(body))
        .build()
    });

  let get_missing = mock!(Client::get_object)
    .then_error(|| GetObjectError::NoSuchKey(NoSuchKey::builder().build()));

  let base = base_path.to_path_buf();
  let matches = base.clone();
  let head_ok = mock!(Client::head_object)
    .match_requests(move |req| object_path(&matches, req.bucket(), req.key()).is_file())
    .then_compute_output(move |req| {
      let len = metadata(object_path(&base, req.bucket(), req.key()))
        .map(|metadata| metadata.len())
        .unwrap_or_default();
      HeadObjectOutput::builder()
        .content_length(len as i64)
        .build()
    });

  let head_missing = mock!(Client::head_object)
    .then_error(|| HeadObjectError::NotFound(NotFound::builder().build()));

  mock_client!(
    aws_sdk_s3,
    RuleMode::MatchAny,
    [&get_ok, &get_missing, &head_ok, &head_missing],
    |config| config
      .endpoint_url(format!("http://{domain_name}"))
      .region(Region::new(region))
      .behavior_version(BehaviorVersion::latest())
  )
}

/// Run a mock s3 server using the `server_base_path` and a test function. Specify the domain name and region to use for the mock server.
pub async fn run_s3_test_server<F, Fut>(
  server_base_path: &Path,
  test: F,
  domain_name: &str,
  region: &'static str,
) where
  F: FnOnce(Client, PathBuf) -> Fut,
  Fut: Future<Output = ()>,
{
  let client = mock_s3_client(server_base_path, domain_name, region);
  test(client, server_base_path.to_path_buf()).await;
}

/// Run a mock s3 server using the `server_base_path` and a test function. Uses the default domain name and region.
pub async fn with_s3_test_server<F, Fut>(server_base_path: &Path, test: F)
where
  F: FnOnce(Client) -> Fut,
  Fut: Future<Output = ()>,
{
  run_s3_test_server(
    server_base_path,
    |client, _| test(client),
    DEFAULT_DOMAIN_NAME,
    DEFAULT_REGION,
  )
  .await;
}

/// Run a mock s3 server. Uses the default domain name and region, and a temporary directory as the base path.
pub async fn with_s3_test_server_tmp<F, Fut>(test: F)
where
  F: FnOnce(Client, PathBuf) -> Fut,
  Fut: Future<Output = ()>,
{
  let tmp_dir = TempDir::new().unwrap();

  run_s3_test_server(tmp_dir.path(), test, DEFAULT_DOMAIN_NAME, DEFAULT_REGION).await;
}
