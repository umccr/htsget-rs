#![cfg(all(feature = "crypt4gh", feature = "url-storage"))]

use base64::engine::general_purpose;
use base64::Engine;
use htsget_config::resolver::object::ObjectType;
use htsget_config::storage::url::endpoints::Endpoints;
use htsget_config::tls::crypt4gh::Crypt4GHKeyPair;
use htsget_config::types::Class::{Body, Header};
use htsget_config::types::Request as HtsgetRequest;
use htsget_config::types::{Format, Query};
use htsget_search::htsget::from_storage::HtsGetFromStorage;
use htsget_search::htsget::HtsGet;
use htsget_search::storage::url::encrypt::Encrypt;
use htsget_search::storage::url::{UrlStorage, CLIENT_PUBLIC_KEY_NAME};
use htsget_test::crypt4gh::{create_local_test_files, expected_key_pair, get_encryption_keys};
use htsget_test::http::server::with_test_server;
use htsget_test::http::{
  default_dir, get_byte_ranges_from_url_storage_response, test_parsable_byte_ranges,
};
use http::header::{AUTHORIZATION, USER_AGENT};
use http::{HeaderMap, HeaderName, HeaderValue, Uri};
use hyper::client::HttpConnector;
use hyper::Client;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use std::future::Future;
use std::str::FromStr;

fn test_client() -> Client<HttpsConnector<HttpConnector>> {
  Client::builder().build(
    HttpsConnectorBuilder::new()
      .with_native_roots()
      .https_or_http()
      .enable_http1()
      .enable_http2()
      .build(),
  )
}

fn test_headers(headers: &mut HeaderMap) -> &HeaderMap {
  headers.append(
    HeaderName::from_str(AUTHORIZATION.as_str()).unwrap(),
    HeaderValue::from_str("secret").unwrap(),
  );
  headers
}

async fn with_url_test_server<F, Fut>(test: F)
where
  F: FnOnce(String) -> Fut,
  Fut: Future<Output = ()>,
{
  let (_, base_path) = create_local_test_files().await;
  with_test_server(base_path.path(), test).await;
}

fn endpoints_from_url_with_path(url: &str) -> Endpoints {
  Endpoints::new(
    Uri::from_str(&format!("{}/endpoint_index", url))
      .unwrap()
      .into(),
    Uri::from_str(&format!("{}/endpoint_file", url))
      .unwrap()
      .into(),
  )
}

#[tokio::test]
async fn test_encrypted_bam() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request =
      HtsgetRequest::new_with_id("htsnexus_test_NA12878".to_string()).with_headers(header_map);
    let query = Query::new(
      "htsnexus_test_NA12878",
      Format::Bam,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    );

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/htsnexus_test_NA12878.bam.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Bam, Body).await;
  })
  .await;
}

#[tokio::test]
async fn test_encrypted_cram() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request =
      HtsgetRequest::new_with_id("htsnexus_test_NA12878".to_string()).with_headers(header_map);
    let query = Query::new(
      "htsnexus_test_NA12878",
      Format::Cram,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    );

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/htsnexus_test_NA12878.cram.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Cram, Body).await;
  })
  .await;
}

#[tokio::test]
async fn test_encrypted_vcf() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request = HtsgetRequest::new_with_id("spec-v4.3".to_string()).with_headers(header_map);
    let query = Query::new(
      "spec-v4.3",
      Format::Vcf,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    );

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/spec-v4.3.vcf.gz.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Vcf, Body).await;
  })
  .await;
}

#[tokio::test]
async fn test_encrypted_bcf() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request =
      HtsgetRequest::new_with_id("sample1-bcbio-cancer".to_string()).with_headers(header_map);
    let query = Query::new(
      "sample1-bcbio-cancer",
      Format::Bcf,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    );

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/sample1-bcbio-cancer.bcf.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Bcf, Body).await;
  })
  .await;
}

#[tokio::test]
async fn test_encrypted_bam_with_range() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request =
      HtsgetRequest::new_with_id("htsnexus_test_NA12878".to_string()).with_headers(header_map);
    let query = Query::new(
      "htsnexus_test_NA12878",
      Format::Bam,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    )
    .with_reference_name("11")
    .with_start(5015000)
    .with_end(5050000);

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/htsnexus_test_NA12878.bam.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Bam, Body).await;
  })
  .await;
}

#[tokio::test]
async fn test_encrypted_cram_with_range() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request =
      HtsgetRequest::new_with_id("htsnexus_test_NA12878".to_string()).with_headers(header_map);
    let query = Query::new(
      "htsnexus_test_NA12878",
      Format::Cram,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    )
    .with_reference_name("11")
    .with_start(5000000)
    .with_end(5050000);

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/htsnexus_test_NA12878.cram.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Cram, Body).await;
  })
  .await;
}

#[tokio::test]
async fn test_encrypted_vcf_with_range() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request = HtsgetRequest::new_with_id("spec-v4.3".to_string()).with_headers(header_map);
    let query = Query::new(
      "spec-v4.3",
      Format::Vcf,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    )
    .with_reference_name("20")
    .with_start(150)
    .with_end(153);

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/spec-v4.3.vcf.gz.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Vcf, Body).await;
  })
  .await;
}

#[tokio::test]
async fn test_encrypted_bcf_with_range() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request =
      HtsgetRequest::new_with_id("sample1-bcbio-cancer".to_string()).with_headers(header_map);
    let query = Query::new(
      "sample1-bcbio-cancer",
      Format::Bcf,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    )
    .with_reference_name("chrM")
    .with_start(150)
    .with_end(153);

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/sample1-bcbio-cancer.bcf.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Bcf, Body).await;
  })
  .await;
}

#[tokio::test]
async fn test_encrypted_bam_header() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request =
      HtsgetRequest::new_with_id("htsnexus_test_NA12878".to_string()).with_headers(header_map);
    let query = Query::new(
      "htsnexus_test_NA12878",
      Format::Bam,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    )
    .with_class(Header);

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/htsnexus_test_NA12878.bam.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Bam, Header).await;
  })
  .await;
}

#[tokio::test]
async fn test_encrypted_cram_header() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request =
      HtsgetRequest::new_with_id("htsnexus_test_NA12878".to_string()).with_headers(header_map);
    let query = Query::new(
      "htsnexus_test_NA12878",
      Format::Cram,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    )
    .with_class(Header);

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/htsnexus_test_NA12878.cram.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Cram, Header).await;
  })
  .await;
}

#[tokio::test]
async fn test_encrypted_vcf_header() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request = HtsgetRequest::new_with_id("spec-v4.3".to_string()).with_headers(header_map);
    let query = Query::new(
      "spec-v4.3",
      Format::Vcf,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    )
    .with_class(Header);

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/spec-v4.3.vcf.gz.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Vcf, Header).await;
  })
  .await;
}

#[tokio::test]
async fn test_encrypted_bcf_header() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request =
      HtsgetRequest::new_with_id("sample1-bcbio-cancer".to_string()).with_headers(header_map);
    let query = Query::new(
      "sample1-bcbio-cancer",
      Format::Bcf,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    )
    .with_class(Header);

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/sample1-bcbio-cancer.bcf.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Bcf, Header).await;
  })
  .await;
}

// The following tests assume the existence of a large Test.1000G file. They are ignored by default.
// Run with `cargo test --all-features -- --ignored --test-threads=1`. It might take a while.
#[ignore]
#[tokio::test]
async fn test_encrypted_large_vcf_chr8_with_range() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request = HtsgetRequest::new_with_id("Test.1000G.phase3.joint.lifted.UMCCR".to_string())
      .with_headers(header_map);
    let query = Query::new(
      "Test.1000G.phase3.joint.lifted.UMCCR",
      Format::Vcf,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    )
    .with_reference_name("chr8")
    .with_start(1000000)
    .with_end(1000100);

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/Test.1000G.phase3.joint.lifted.UMCCR.vcf.gz.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Vcf, Header).await;
  })
  .await;
}

#[ignore]
#[tokio::test]
async fn test_encrypted_large_vcf_chr2_no_range() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request = HtsgetRequest::new_with_id("Test.1000G.phase3.joint.lifted.UMCCR".to_string())
      .with_headers(header_map);
    let query = Query::new(
      "Test.1000G.phase3.joint.lifted.UMCCR",
      Format::Vcf,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    )
    .with_reference_name("chr2");

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/Test.1000G.phase3.joint.lifted.UMCCR.vcf.gz.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Vcf, Header).await;
  })
  .await;
}

#[ignore]
#[tokio::test]
async fn test_encrypted_large_vcf_chr20_no_end_range() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request = HtsgetRequest::new_with_id("Test.1000G.phase3.joint.lifted.UMCCR".to_string())
      .with_headers(header_map);
    let query = Query::new(
      "Test.1000G.phase3.joint.lifted.UMCCR",
      Format::Vcf,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    )
    .with_reference_name("chr20")
    .with_start(10000000);

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/Test.1000G.phase3.joint.lifted.UMCCR.vcf.gz.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Vcf, Header).await;
  })
  .await;
}

#[ignore]
#[tokio::test]
async fn test_encrypted_large_vcf_chr11_no_start_range() {
  with_url_test_server(|url| async move {
    let (_, public_key) = get_encryption_keys().await;
    let mut header_map = HeaderMap::default();
    let public_key = general_purpose::STANDARD.encode(public_key);
    test_headers(&mut header_map);
    header_map.append(
      HeaderName::from_str(CLIENT_PUBLIC_KEY_NAME).unwrap(),
      HeaderValue::from_str(&public_key).unwrap(),
    );
    header_map.append(
      HeaderName::from_str(USER_AGENT.as_ref()).unwrap(),
      HeaderValue::from_str("client-user-agent").unwrap(),
    );

    let request = HtsgetRequest::new_with_id("Test.1000G.phase3.joint.lifted.UMCCR".to_string())
      .with_headers(header_map);
    let query = Query::new(
      "Test.1000G.phase3.joint.lifted.UMCCR",
      Format::Vcf,
      request,
      ObjectType::Crypt4GH {
        crypt4gh: Crypt4GHKeyPair::new(expected_key_pair()),
        send_encrypted_to_client: true,
      },
    )
    .with_reference_name("chr11")
    .with_end(50000);

    let storage = UrlStorage::new(
      test_client(),
      endpoints_from_url_with_path(&url),
      Uri::from_str("http://example.com").unwrap(),
      true,
      Some("user-agent".to_string()),
      &query,
      Encrypt,
    )
    .unwrap();

    let searcher = HtsGetFromStorage::new(storage);
    let response = searcher.search(query.clone()).await.unwrap();

    let (bytes, _) = get_byte_ranges_from_url_storage_response(
      response,
      default_dir().join("data/crypt4gh/Test.1000G.phase3.joint.lifted.UMCCR.vcf.gz.c4gh"),
    )
    .await;

    test_parsable_byte_ranges(bytes.clone(), Format::Vcf, Header).await;
  })
  .await;
}
