//! Local Crypt4GH storage access.
//!

use crate::c4gh::edit::{ClampedPosition, EditHeader, UnencryptedPosition};
use crate::c4gh::{
  to_unencrypted_file_size, unencrypted_clamp, unencrypted_clamp_next, unencrypted_to_data_block,
  unencrypted_to_next_data_block, DeserializedHeader,
};
use crate::error::StorageError::{InternalError, IoError};
use crate::error::{Result, StorageError};
use crate::types::BytesPosition;
use crate::{
  BytesPositionOptions, DataBlock, GetOptions, HeadOptions, RangeUrlOptions, StorageMiddleware,
  StorageTrait, Streamable,
};
use async_trait::async_trait;
use crypt4gh::error::Crypt4GHError;
use crypt4gh::Keys;
use htsget_config::types::{Class, Format, Url};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::io;
use std::io::{BufReader, Cursor};
use tokio::io::AsyncReadExt;

/// Max C4GH header size in bytes. Supports 50 regular sized encrypted packets. 16 + (108 * 50).
const MAX_C4GH_HEADER_SIZE: u64 = 5416;

/// This represents the state that the C4GHStorage needs to save, like the file sizes and header
/// sizes.
#[derive(Debug, Clone)]
pub struct C4GHState {
  encrypted_file_size: u64,
  unencrypted_file_size: u64,
  deserialized_header: DeserializedHeader,
}

/// Implementation for the [StorageTrait] trait using the local file system for accessing Crypt4GH
/// encrypted files. [T] is the type of the server struct, which is used for formatting urls.
pub struct C4GHStorage {
  keys: Vec<Keys>,
  inner: Box<dyn StorageTrait + Send + Sync + 'static>,
  state: HashMap<String, C4GHState>,
}

impl Clone for C4GHStorage {
  fn clone(&self) -> Self {
    Self {
      keys: self.keys.clone(),
      inner: self.inner.clone_box(),
      state: self.state.clone(),
    }
  }
}

impl Debug for C4GHStorage {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    write!(f, "C4GHStorage")
  }
}

impl C4GHStorage {
  /// Create a new storage from a storage trait.
  pub fn new(keys: Vec<Keys>, inner: impl StorageTrait + Send + Sync + 'static) -> Self {
    Self {
      keys,
      inner: Box::new(inner),
      state: Default::default(),
    }
  }

  /// Format a C4GH key.
  pub fn format_key(key: &str) -> String {
    format!("{}.c4gh", key)
  }

  /// Get a C4GH object and decrypt it if it is not an index.
  pub async fn get_object(&self, key: &str, options: GetOptions<'_>) -> Result<Streamable> {
    if Format::is_index(key) {
      return self.inner.get(key, options).await;
    }

    let data = self
      .state
      .get(&Self::format_key(key))
      .ok_or_else(|| InternalError("missing key from state".to_string()))?
      .clone();

    Ok(Streamable::from_async_read(Cursor::new(
      data.deserialized_header.decrypted_stream,
    )))
  }

  /// Get the size of the unencrypted object and update state.
  pub async fn preprocess_for_state(
    &mut self,
    key: &str,
    mut options: GetOptions<'_>,
  ) -> Result<u64> {
    if Format::is_index(key) {
      return self.inner.head(key, (&options).into()).await;
    }

    let key = Self::format_key(key);

    // Get the file size.
    let encrypted_file_size = self.inner.head(&key, (&options).into()).await?;

    let end = options
      .range
      .end
      .unwrap_or_default()
      .checked_add(MAX_C4GH_HEADER_SIZE)
      .ok_or_else(|| InternalError("overflow getting header".to_string()))?;
    options.range = options.range.with_end(end);

    // Also need to determine the header size.
    let mut buf = vec![];
    self
      .inner
      .get(&key, options)
      .await?
      .read_to_end(&mut buf)
      .await?;

    let mut reader = BufReader::new(Cursor::new(buf));

    let deserialized_header = DeserializedHeader::from_buffer(&mut reader, &self.keys)?;
    let unencrypted_file_size =
      to_unencrypted_file_size(encrypted_file_size, deserialized_header.header_size);

    let state = C4GHState {
      encrypted_file_size,
      unencrypted_file_size,
      deserialized_header,
    };

    self.state.insert(key, state);

    Ok(unencrypted_file_size)
  }

  /// Compute the data blocks including edit lists, additional data encryption packets, and encrypted bytes.
  pub async fn compute_data_blocks(
    &self,
    key: &str,
    options: BytesPositionOptions<'_>,
  ) -> Result<Vec<DataBlock>> {
    let state = self
      .state
      .get(&Self::format_key(key))
      .ok_or_else(|| InternalError("missing key from state".to_string()))?;

    let default_start = |pos: &BytesPosition| pos.start.unwrap_or_default();
    let default_end = |pos: &BytesPosition| pos.end.unwrap_or(state.unencrypted_file_size);

    let header_size = state.deserialized_header.header_size;
    let encrypted_file_size = state.encrypted_file_size;

    // Original positions.
    let mut unencrypted_positions = vec![];
    // Positions from the reference frame of creating an edit list with discards/keep bytes.
    let mut clamped_positions = vec![];
    // Positions from the reference frame of someone merging bytes from htsget.
    let mut encrypted_positions = vec![];
    for mut pos in options.positions {
      let start = default_start(&pos);
      let end = default_end(&pos);

      pos.start = Some(start);
      pos.end = Some(end);
      unencrypted_positions.push(pos.clone());

      pos.start = Some(unencrypted_clamp(start, header_size, encrypted_file_size));
      pos.end = Some(unencrypted_clamp_next(
        end,
        header_size,
        encrypted_file_size,
      ));
      clamped_positions.push(pos.clone());

      pos.start = Some(unencrypted_to_data_block(
        start,
        header_size,
        encrypted_file_size,
      ));
      pos.end = Some(unencrypted_to_next_data_block(
        end,
        header_size,
        encrypted_file_size,
      ));
      encrypted_positions.push(pos);
    }

    let unencrypted_positions = BytesPosition::merge_all(unencrypted_positions)
      .into_iter()
      .map(|pos| UnencryptedPosition::new(default_start(&pos), default_end(&pos)))
      .collect::<Vec<_>>();
    let clamped_positions = BytesPosition::merge_all(clamped_positions)
      .into_iter()
      .map(|pos| ClampedPosition::new(default_start(&pos), default_end(&pos)))
      .collect::<Vec<_>>();

    let (header_info, reencrypted_bytes, edit_list_packet) = EditHeader::new(
      unencrypted_positions,
      clamped_positions,
      &self.keys,
      &state.deserialized_header,
    )
    .reencrypt_header()?
    .into_inner();

    let header_info_size = header_info.len() as u64;
    let current_header_size = state.deserialized_header.header_size;
    let mut blocks = vec![
      DataBlock::Data(header_info, Some(Class::Header)),
      DataBlock::Range(
        BytesPosition::default()
          .with_start(header_info_size)
          .with_end(current_header_size),
      ),
      DataBlock::Data(
        [edit_list_packet, reencrypted_bytes].concat(),
        Some(Class::Header),
      ),
    ];

    blocks.extend(DataBlock::from_bytes_positions(BytesPosition::merge_all(
      encrypted_positions,
    )));

    Ok(blocks)
  }
}

#[async_trait]
impl StorageMiddleware for C4GHStorage {
  async fn preprocess(&mut self, key: &str, options: GetOptions<'_>) -> Result<()> {
    self.preprocess_for_state(key, options).await?;
    Ok(())
  }

  async fn postprocess(
    &self,
    key: &str,
    positions_options: BytesPositionOptions<'_>,
  ) -> Result<Vec<DataBlock>> {
    self.compute_data_blocks(key, positions_options).await
  }
}

#[async_trait]
impl StorageTrait for C4GHStorage {
  /// Get the Crypt4GH file at the location of the key.
  async fn get(&self, key: &str, options: GetOptions<'_>) -> Result<Streamable> {
    self.get_object(key, options).await
  }

  /// Get a url for the file at key. This refers to the underlying `StorageTrait`.
  async fn range_url(&self, key: &str, options: RangeUrlOptions<'_>) -> Result<Url> {
    self.inner.range_url(&Self::format_key(key), options).await
  }

  /// Get the size of the underlying file and the encrypted file, updating any state.
  async fn head(&self, key: &str, _options: HeadOptions<'_>) -> Result<u64> {
    Ok(
      self
        .state
        .get(&Self::format_key(key))
        .ok_or_else(|| InternalError("failed to call preprocess".to_string()))?
        .unencrypted_file_size,
    )
  }
}

impl From<Crypt4GHError> for StorageError {
  fn from(err: Crypt4GHError) -> Self {
    IoError("Crypt4GH".to_string(), io::Error::other(err))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::local::tests::with_local_storage;
  use htsget_config::types::Headers;
  use htsget_test::c4gh::{encrypt_data, get_decryption_keys};
  use std::future::Future;
  use tokio::fs::File;
  use tokio::io::AsyncWriteExt;

  #[tokio::test]
  async fn test_preprocess() {
    with_c4gh_storage(|mut storage| async move {
      storage
        .preprocess(
          "key",
          GetOptions::new_with_default_range(&Default::default()),
        )
        .await
        .unwrap();

      let state = storage.state.get("key.c4gh").unwrap();

      assert_eq!(state.unencrypted_file_size, 6);
      assert_eq!(state.encrypted_file_size, 158);
      assert_eq!(state.deserialized_header.header_info.packets_count, 1);
    })
    .await;
  }

  #[tokio::test]
  async fn test_get() {
    with_c4gh_storage(|mut storage| async move {
      let headers = Default::default();
      let options = GetOptions::new_with_default_range(&headers);
      storage.preprocess("key", options.clone()).await.unwrap();
      let mut object = vec![];

      storage
        .get("key", options)
        .await
        .unwrap()
        .read_to_end(&mut object)
        .await
        .unwrap();
      assert_eq!(object, b"value1");
    })
    .await;
  }

  #[tokio::test]
  async fn test_head() {
    with_c4gh_storage(|mut storage| async move {
      let headers = Default::default();
      let options = GetOptions::new_with_default_range(&headers);
      storage.preprocess("key", options.clone()).await.unwrap();

      let size = storage
        .head("key", HeadOptions::new(&headers))
        .await
        .unwrap();
      assert_eq!(size, 6);
    })
    .await;
  }

  #[tokio::test]
  async fn test_postprocess() {
    with_c4gh_storage(|mut storage| async move {
      let headers = Default::default();
      let options = GetOptions::new_with_default_range(&headers);
      storage.preprocess("key", options.clone()).await.unwrap();

      let blocks = storage
        .postprocess(
          "key",
          BytesPositionOptions::new(
            vec![BytesPosition::default().with_start(0).with_end(6)],
            &headers,
          ),
        )
        .await
        .unwrap();

      assert_eq!(
        blocks[0],
        DataBlock::Data(
          vec![99, 114, 121, 112, 116, 52, 103, 104, 1, 0, 0, 0, 3, 0, 0, 0],
          Some(Class::Header)
        )
      );
      assert_eq!(
        blocks[1],
        DataBlock::Range(BytesPosition::new(Some(16), Some(124), None))
      );
      assert_eq!(
        blocks[3],
        DataBlock::Range(BytesPosition::new(Some(124), Some(158), None))
      );
    })
    .await;
  }

  #[tokio::test]
  async fn test_range_url() {
    with_c4gh_storage(|mut storage| async move {
      let headers = Default::default();
      let options = GetOptions::new_with_default_range(&headers);
      storage.preprocess("key", options.clone()).await.unwrap();

      let blocks = storage
        .postprocess(
          "key",
          BytesPositionOptions::new(
            vec![BytesPosition::default().with_start(0).with_end(6)],
            &headers,
          ),
        )
        .await
        .unwrap();

      if let DataBlock::Range(range) = blocks.last().unwrap() {
        let url = storage
          .range_url("key", RangeUrlOptions::new(range.clone(), &headers))
          .await
          .unwrap();
        let expected = Url::new("http://127.0.0.1:8081/data/key.c4gh")
          .with_headers(Headers::default().with_header("Range", "bytes=124-157"));

        assert_eq!(url, expected);
      }
    })
    .await;
  }

  pub(crate) async fn with_c4gh_storage<F, Fut>(test: F)
  where
    F: FnOnce(C4GHStorage) -> Fut,
    Fut: Future<Output = ()>,
  {
    with_local_storage(|storage| async move {
      let mut data = vec![];
      StorageTrait::get(
        &storage,
        "folder/../key1",
        GetOptions::new_with_default_range(&Default::default()),
      )
      .await
      .unwrap()
      .read_to_end(&mut data)
      .await
      .unwrap();

      let data = encrypt_data(&data);

      let key = "key.c4gh";
      File::create(storage.base_path().join(key))
        .await
        .unwrap()
        .write_all(&data)
        .await
        .unwrap();

      test(C4GHStorage::new(get_decryption_keys(), storage)).await;
    })
    .await;
  }
}
