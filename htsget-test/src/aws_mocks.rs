use aws_config::SdkConfig;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_credential_types::Credentials;
use aws_sdk_s3::{Client, Region};
use s3s::service::S3Service;
use std::future::Future;
use std::path::Path;
use tempfile::TempDir;

/// Default domain to use for mock s3 server
pub const DEFAULT_DOMAIN_NAME: &str = "localhost:8014";
/// Default region to use for mock s3 server
pub const DEFAULT_REGION: &str = "ap-southeast-2";

/// Run a mock s3 server using the `server_base_path` and a test function. Specify the domain name and region to use for the mock server.
pub async fn run_s3_test_server<F, Fut>(
  server_base_path: &Path,
  test: F,
  domain_name: &'static str,
  region: &'static str,
) where
  F: FnOnce(Client, &Path) -> Fut,
  Fut: Future<Output = ()>,
{
  let cred = Credentials::for_tests();

  let conn = {
    let fs = s3s_fs::FileSystem::new(server_base_path).unwrap();

    let auth = s3s::SimpleAuth::from_single(cred.access_key_id(), cred.secret_access_key());

    let mut service = S3Service::new(Box::new(fs));
    service.set_auth(Box::new(auth));
    service.set_base_domain(domain_name);

    s3s_aws::Connector::from(service.into_shared())
  };

  let sdk_config = SdkConfig::builder()
    .credentials_provider(SharedCredentialsProvider::new(cred))
    .http_connector(conn)
    .region(Region::new(region))
    .endpoint_url(format!("http://{domain_name}"))
    .build();

  test(Client::new(&sdk_config), server_base_path).await;
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
  F: FnOnce(Client, &Path) -> Fut,
  Fut: Future<Output = ()>,
{
  let tmp_dir = TempDir::new().unwrap();

  run_s3_test_server(tmp_dir.path(), test, DEFAULT_DOMAIN_NAME, DEFAULT_REGION).await;
}
