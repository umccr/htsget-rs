use axum::middleware::Next;
use axum::response::Response;
use crypt4gh::keys::{get_private_key, get_public_key};
use crypt4gh::Keys;
use http::header::AUTHORIZATION;
use http::{Request, StatusCode};
use tempfile::TempDir;
use tokio::fs::{create_dir, File};
use tokio::io::AsyncWriteExt;
use tokio_rustls::rustls::PrivateKey;

use async_crypt4gh::{KeyPair, PublicKey};

use crate::http::get_test_path;

/// Returns the private keys of the recipient and the senders public key from the context of decryption.
pub async fn get_decryption_keys() -> (Keys, Vec<u8>) {
  get_keys("crypt4gh/keys/bob.sec", "crypt4gh/keys/alice.pub").await
}

/// Returns the private keys of the recipient and the senders public key from the context of encryption.
pub async fn get_encryption_keys() -> (Keys, Vec<u8>) {
  get_keys("crypt4gh/keys/alice.sec", "crypt4gh/keys/bob.pub").await
}

/// Get the crypt4gh keys from the paths.
pub async fn get_keys(private_key: &str, public_key: &str) -> (Keys, Vec<u8>) {
  let private_key = get_private_key(get_test_path(private_key), Ok("".to_string())).unwrap();
  let public_key = get_public_key(get_test_path(public_key)).unwrap();

  (
    Keys {
      method: 0,
      privkey: private_key,
      recipient_pubkey: public_key.clone(),
    },
    public_key,
  )
}

pub fn expected_key_pair() -> KeyPair {
  KeyPair::new(
    PrivateKey(vec![
      162, 124, 25, 18, 207, 218, 241, 41, 162, 107, 29, 40, 10, 93, 30, 193, 104, 42, 176, 235,
      207, 248, 126, 230, 97, 205, 253, 224, 215, 160, 67, 239,
    ]),
    PublicKey::new(vec![
      56, 44, 122, 180, 24, 116, 207, 149, 165, 49, 204, 77, 224, 136, 232, 121, 209, 249, 23, 51,
      120, 2, 187, 147, 82, 227, 232, 32, 17, 223, 7, 38,
    ]),
  )
}

pub async fn test_auth<B>(request: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
  let auth_header = request
    .headers()
    .get(AUTHORIZATION)
    .and_then(|header| header.to_str().ok());

  match auth_header {
    Some("secret") => Ok(next.run(request).await),
    _ => Err(StatusCode::UNAUTHORIZED),
  }
}

pub async fn create_local_test_files() -> (String, TempDir) {
  let base_path = TempDir::new().unwrap();

  let folder_name = "folder";
  let key1 = "key1";
  let value1 = b"value1";
  let key2 = "key2";
  let value2 = b"value2";
  File::create(base_path.path().join(key1))
    .await
    .unwrap()
    .write_all(value1)
    .await
    .unwrap();
  create_dir(base_path.path().join(folder_name))
    .await
    .unwrap();
  File::create(base_path.path().join(folder_name).join(key2))
    .await
    .unwrap()
    .write_all(value2)
    .await
    .unwrap();

  (folder_name.to_string(), base_path)
}
