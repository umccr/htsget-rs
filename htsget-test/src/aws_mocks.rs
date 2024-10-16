use aws_config::SdkConfig;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::{BehaviorVersion, Region};
use aws_sdk_s3::Client;
use s3s::auth::SimpleAuth;
use s3s::service::S3ServiceBuilder;
use s3s_fs::FileSystem;
use std::future::Future;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Default domain to use for mock s3 server.
pub const DEFAULT_DOMAIN_NAME: &str = "localhost:0";

/// Default region to use for mock s3 server.
pub const DEFAULT_REGION: &str = "ap-southeast-2";

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
  let cred = Credentials::for_tests();

  let client = {
    let fs = FileSystem::new(server_base_path).unwrap();

    let auth = SimpleAuth::from_single(cred.access_key_id(), cred.secret_access_key());

    let mut service = S3ServiceBuilder::new(fs);
    service.set_auth(auth);
    service.set_base_domain(domain_name);

    s3s_aws::Client::from(service.build().into_shared())
  };

  let sdk_config = SdkConfig::builder()
    .credentials_provider(SharedCredentialsProvider::new(cred))
    .http_client(client)
    .region(Region::new(region))
    .endpoint_url(format!("http://{domain_name}"))
    .behavior_version(BehaviorVersion::latest())
    .build();

  test(Client::new(&sdk_config), server_base_path.to_path_buf()).await;
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
