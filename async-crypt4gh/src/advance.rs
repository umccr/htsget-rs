use async_trait::async_trait;
use std::io;

/// A trait which defines the advance operation.
///
/// Advance to an offset, in bytes, in the stream. This is very similar to seek, but it only
/// operates on the current stream and it does not change the position of the underlying stream
/// or buffer.
///
/// This is useful for implementing seek-like operations on data types where information about
/// a stream's position can be obtained without having access to the whole stream. For example,
/// determining the offsets of data blocks in a Crypt4GH file while only having access to the
/// header and the file's size.
#[async_trait]
pub trait Advance {
  /// Advance in the encrypted stream. This function returns the new position of the
  /// advanced stream.
  async fn advance_encrypted(&mut self, position: u64) -> io::Result<u64>;

  /// Advance in the unencrypted stream. This function returns the new position of the
  /// advanced stream.
  async fn advance_unencrypted(&mut self, position: u64) -> io::Result<u64>;

  /// Get the stream length, if it is available.
  fn stream_length(&self) -> Option<u64>;
}
