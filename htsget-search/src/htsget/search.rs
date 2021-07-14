use std::io;

use noodles_bgzf::VirtualPosition;

use crate::{
  htsget::{Class, Format, HtsGetError, Query, Response, Result, Url},
  storage::{BytesRange, Storage, UrlOptions},
};

const MAX_BLOCK_SIZE: u64 = 65536;

pub(crate) trait Search<'a, S, T>
  where S: Storage + 'a
{
  /// This returns mapped and placed unmapped ranges
  fn get_byte_ranges_for_all(
    &self,
    key: &str,
    index: &T,
  ) -> Result<Vec<BytesRange>>;

  /// This returns reads for a given reference name and an optional sequence range
  fn get_byte_ranges_for_reference_name(
    &self,
    key: &str,
    reference_name: &str,
    index: &T,
    query: &Query,
  ) -> Result<Vec<BytesRange>>;

  /// Read index from key
  fn read_index(&self, key: &str) -> Result<T>;

  /// Generate a key for the storage object from an ID
  /// This may involve a more complex transformation in the future,
  /// or even require custom implementations depending on the organizational structure
  /// For now there is a 1:1 mapping to the underlying files
  fn get_keys_from_id(&self, id: &str) -> (String, String);

  /// Returns the header bytes range.
  fn get_byte_ranges_for_header(&self, key: &str) -> Result<Vec<BytesRange>>;

  /// Get the underlying storage of this trait.
  fn get_storage(&self) -> &S;

  /// Get the underlying format of this trait.
  fn get_format(&self) -> Format;

  /// Search based on the query.
  fn search(
    &self,
    query: Query
  ) -> Result<Response> {
    let (file_key, index_key) = self.get_keys_from_id(query.id.as_str());

    match query.class {
      Class::Body => {
        let index = self.read_index(&index_key)?;

        let byte_ranges = match query.reference_name.as_ref() {
          None => self.get_byte_ranges_for_all(file_key.as_str(), &index)?,
          Some(reference_name) => self.get_byte_ranges_for_reference_name(
            file_key.as_str(),
            reference_name,
            &index,
            &query,
          )?,
        };
        self.build_response(query, &file_key, byte_ranges)
      }
      Class::Header => {
        let byte_ranges = self.get_byte_ranges_for_header(&file_key)?;
        self.build_response(query, &file_key, byte_ranges)
      }
    }
  }

  /// Build the response from the query using urls.
  fn build_response(
    &self,
    query: Query,
    key: &str,
    byte_ranges: Vec<BytesRange>
  ) -> Result<Response> {
    let urls = byte_ranges
      .into_iter()
      .map(|range| {
        let options = UrlOptions::default()
          .with_range(range)
          .with_class(query.class.clone());
        self.get_storage()
          .url(&key, options)
          .map_err(HtsGetError::from)
      })
      .collect::<Result<Vec<Url>>>()?;

    let format = query.format.unwrap_or_else(|| self.get_format());
    Ok(Response::new(format, urls))
  }
}

pub(crate) trait BlockPosition {
  /// Read bytes of record.
  fn read_bytes(&mut self) -> Option<usize>;
  /// Seek using VirtualPosition.
  fn seek(&mut self, pos: VirtualPosition) -> io::Result<VirtualPosition>;
  /// Read the virtual position.
  fn virtual_position(&self) -> VirtualPosition;
}

pub(crate) trait VirtualPositionExt {
  /// Get the starting bytes for a compressed BGZF block.
  fn bytes_range_start(&self) -> u64;
  /// Get the ending bytes for a compressed BGZF block.
  fn bytes_range_end(&self, reader: &mut dyn BlockPosition) -> u64;
  fn to_string(&self) -> String;
  fn get_next_block_position(&self, reader: &mut dyn BlockPosition) -> Option<u64>;
}

impl VirtualPositionExt for VirtualPosition {
  /// This is just an alias to compressed. Kept for consistency.
  fn bytes_range_start(&self) -> u64 {
    self.compressed()
  }
  /// The compressed part refers always to the beginning of a BGZF block.
  /// But when we need to translate it into a byte range, we need to make sure
  /// the reads falling inside that block are also included, which requires to know
  /// where that block ends, which is not trivial nor possible for the last block.
  /// The solution used here goes through reading the records starting at the compressed
  /// virtual offset (coffset) of the end position (remember this will always be the
  /// start of a BGZF block). If we read the records pointed by that coffset until we
  /// reach a different coffset, we can find out where the current block ends.
  /// Therefore this can be used to only add the required bytes in the query results.
  /// If for some reason we can't read correctly the records we fall back
  /// to adding the maximum BGZF block size.
  fn bytes_range_end(&self, reader: &mut dyn BlockPosition) -> u64 {
    if self.uncompressed() == 0 {
      // If the uncompressed part is exactly zero, we don't need the next block
      return self.compressed();
    }
    self.get_next_block_position(reader).unwrap_or(self.compressed() + MAX_BLOCK_SIZE)
  }

  fn to_string(&self) -> String {
    format!("{}/{}", self.compressed(), self.uncompressed())
  }

  fn get_next_block_position(&self, reader: &mut dyn BlockPosition) -> Option<u64> {
    reader.seek(*self).ok()?;
    let next_block_index = loop {
      let bytes_read = reader.read_bytes()?;
      let actual_block_index = reader.virtual_position().compressed();
      if bytes_read == 0 || actual_block_index > self.compressed() {
        break actual_block_index;
      }
    };
    Some(next_block_index)
  }
}