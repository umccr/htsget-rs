use crypt4gh::Keys;
use tokio::io::AsyncBufRead;

#[derive(Debug)]
pub struct SenderPublicKey {
    bytes: Vec<u8>
}

pub trait Crypt4gh {
    type Streamable: AsyncBufRead + Unpin + Send + Sync;

    /// Decrypts the header of the underlying file.
    fn decrypt_header(&self, encrypted_data: Self::Streamable, private_keys: Keys, sender_public_key: SenderPublicKey) -> Self::Streamable {

        
        crypt4gh::decrypt(keys, read_buffer, write_buffer, range_start, range_span, sender_pubkey)
    }
}