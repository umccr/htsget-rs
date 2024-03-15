use crypt4gh::keys::{get_private_key, get_public_key};
use crypt4gh::Keys;

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
