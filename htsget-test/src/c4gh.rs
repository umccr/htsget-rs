use crate::util::default_dir;
use crypt4gh::keys::{get_private_key, get_public_key};
use crypt4gh::{Keys, decrypt, encrypt};
use htsget_config::storage::c4gh::{
  C4GHKeyLocation, C4GHKeySet, C4GHKeyType, C4GHKeys, local::C4GHLocal,
};
use std::collections::HashSet;
use std::fs;
use std::io::{BufReader, BufWriter, Cursor};
use std::path::PathBuf;

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

fn create_key_set(private_key: PathBuf, public_key: PathBuf) -> C4GHKeys {
  let private = C4GHKeyType::new_file(C4GHLocal::new(private_key));
  let public = C4GHKeyType::new_file(C4GHLocal::new(public_key));

  C4GHKeys::try_from(C4GHKeySet::new(
    C4GHKeyLocation::new(Some(private.clone()), public.clone()),
    C4GHKeyLocation::new(None, public),
    true,
  ))
  .unwrap()
}

pub async fn get_decryption_keys() -> Vec<Keys> {
  let private_key = default_dir().join("data/c4gh/keys/bob.sec");
  let public_key = default_dir().join("data/c4gh/keys/bob.pub");

  create_key_set(private_key, public_key)
    .into_inner()
    .await
    .unwrap()
    .0
}

pub async fn get_encryption_keys() -> Vec<Keys> {
  let private_key = default_dir().join("data/c4gh/keys/bob.sec");
  let public_key = default_dir().join("data/c4gh/keys/alice.pub");

  create_key_set(private_key, public_key)
    .into_inner()
    .await
    .unwrap()
    .1
}

pub fn get_encoded_public_key() -> String {
  let public_key = default_dir().join("data/c4gh/keys/bob.pub");
  fs::read_to_string(public_key).unwrap()
}
