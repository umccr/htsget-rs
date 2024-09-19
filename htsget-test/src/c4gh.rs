use crate::util::default_dir;
use crypt4gh::keys::{get_private_key, get_public_key};
use crypt4gh::{decrypt, encrypt, Keys};
use htsget_config::storage::object::c4gh::{C4GHKeys, C4GHPath};
use std::collections::HashSet;
use std::io::{BufReader, BufWriter, Cursor};

pub fn decrypt_data(data: &[u8]) -> Vec<u8> {
  let keys = get_private_key(
    default_dir().join("data/c4gh/keys/alice.sec"),
    Ok("".to_string()),
  )
  .unwrap();

  let mut reader = BufReader::new(Cursor::new(data));
  let mut writer = BufWriter::new(Cursor::new(vec![]));

  decrypt(
    &[Keys {
      method: 0,
      privkey: keys,
      recipient_pubkey: vec![],
    }],
    &mut reader,
    &mut writer,
    0,
    None,
    &None,
  )
  .unwrap();

  writer.into_inner().unwrap().into_inner()
}

pub fn encrypt_data(data: &[u8]) -> Vec<u8> {
  let keys = get_private_key(
    default_dir().join("data/c4gh/keys/alice.sec"),
    Ok("".to_string()),
  )
  .unwrap();
  let recipient_key = get_public_key(default_dir().join("data/c4gh/keys/bob.pub")).unwrap();

  let mut reader = BufReader::new(Cursor::new(data));
  let mut writer = BufWriter::new(Cursor::new(vec![]));

  encrypt(
    &HashSet::from_iter(vec![Keys {
      method: 0,
      privkey: keys,
      recipient_pubkey: recipient_key,
    }]),
    &mut reader,
    &mut writer,
    0,
    None,
  )
  .unwrap();

  writer.into_inner().unwrap().into_inner()
}

pub fn get_decryption_keys() -> Vec<Keys> {
  let private_key = default_dir().join("data/c4gh/keys/bob.sec");
  let public_key = default_dir().join("data/c4gh/keys/alice.pub");
  let keys = C4GHKeys::try_from(C4GHPath::new(private_key, public_key)).unwrap();

  keys.into_inner()
}
