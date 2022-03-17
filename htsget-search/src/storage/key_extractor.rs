use crate::htsget::Format;
use super::Result;

/// The KeyExtractor trait allows getting the index and file keys associated with a particular
/// id and format.
pub trait KeyExtractor<K> {
  fn get_index_key<T: AsRef<str>>(id: T, format: Format) -> Result<K>;
  fn get_file_key<T: AsRef<str>>(id: T, format: Format) -> Result<K>;
}
