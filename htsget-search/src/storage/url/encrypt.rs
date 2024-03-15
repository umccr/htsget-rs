use crate::storage::url::UrlStreamReader;
use crate::storage::Result;
use crate::storage::StorageError::UrlParseError;
use async_crypt4gh::edit_lists::{ClampedPosition, EditHeader, UnencryptedPosition};
use async_crypt4gh::reader::Reader;
use async_crypt4gh::{util, KeyPair, PublicKey};
use mockall::mock;
use std::fmt::{Debug, Formatter};
use tokio_rustls::rustls::PrivateKey;

/// A wrapper around url storage encryption.
#[derive(Debug, Clone, Default)]
pub struct Encrypt;

impl Encrypt {
  pub fn new_with_defaults() -> Self {
    Self
  }

  pub fn generate_key_pair(&self) -> Result<KeyPair> {
    util::generate_key_pair().map_err(|err| UrlParseError(err.to_string()))
  }

  pub fn edit_list(
    &self,
    reader: &Reader<UrlStreamReader>,
    unencrypted_positions: Vec<UnencryptedPosition>,
    clamped_positions: Vec<ClampedPosition>,
    private_key: PrivateKey,
    public_key: PublicKey,
  ) -> Result<(Vec<u8>, Vec<u8>)> {
    let (header_info, _, edit_list_packet) = EditHeader::new(
      reader,
      unencrypted_positions,
      clamped_positions,
      private_key,
      public_key,
    )
    .edit_list()
    .map_err(|err| UrlParseError(err.to_string()))?
    .ok_or_else(|| UrlParseError("crypt4gh header has not been read".to_string()))?
    .into_inner();

    Ok((header_info, edit_list_packet))
  }
}

mock! {
    pub Encrypt {
        pub fn generate_key_pair(&self) -> Result<KeyPair>;

        pub fn edit_list(
            &self,
            reader: &Reader<UrlStreamReader>,
            unencrypted_positions: Vec<UnencryptedPosition>,
            clamped_positions: Vec<ClampedPosition>,
            private_key: PrivateKey,
            public_key: PublicKey,
        ) -> Result<(Vec<u8>, Vec<u8>)>;
    }

    impl Clone for Encrypt {
        fn clone(&self) -> Self;
    }

    impl Debug for Encrypt {
        fn fmt<'a>(&self, f: &mut Formatter<'a>) -> std::fmt::Result;
    }
}
