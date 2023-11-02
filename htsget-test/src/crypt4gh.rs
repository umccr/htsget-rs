use crate::http_tests::get_test_path;
use crypt4gh::keys::{get_private_key, get_public_key};
use crypt4gh::Keys;

/// Get the crypt4gh keys.
pub async fn get_keys() -> (Keys, Vec<u8>) {
  let recipient_private_key =
    get_private_key(get_test_path("crypt4gh/keys/bob.sec"), Ok("".to_string())).unwrap();
  let sender_public_key = get_public_key(get_test_path("crypt4gh/keys/alice.pub")).unwrap();

  (
    Keys {
      method: 0,
      privkey: recipient_private_key,
      recipient_pubkey: sender_public_key.clone(),
    },
    sender_public_key,
  )
}
