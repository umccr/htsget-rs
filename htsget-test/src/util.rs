use std::fs;
use std::path::{Path, PathBuf};

use rcgen::{KeyPair, generate_simple_self_signed};

/// Generate test certificates.
pub fn generate_test_certificates<P: AsRef<Path>>(
  in_path: P,
  key_name: &str,
  cert_name: &str,
) -> (PathBuf, PathBuf) {
  let key_path = in_path.as_ref().join(key_name);
  let cert_path = in_path.as_ref().join(cert_name);

  let cert = generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
  fs::write(&key_path, cert.signing_key.serialize_pem()).unwrap();
  fs::write(&cert_path, cert.cert.pem()).unwrap();

  (key_path, cert_path)
}

/// Generate a public and private key pair.
pub fn generate_key_pair<P: AsRef<Path>>(
  in_path: P,
  private_key: &str,
  public_key: &str,
) -> (Vec<u8>, Vec<u8>) {
  let private_key = in_path.as_ref().join(private_key);
  let public_key = in_path.as_ref().join(public_key);

  let key_pair = KeyPair::generate().unwrap();
  let private_key_pem = key_pair.serialize_pem();
  let public_key_pem = key_pair.public_key_pem();

  fs::write(&private_key, &private_key_pem).unwrap();
  fs::write(&public_key, &public_key_pem).unwrap();

  (private_key_pem.into_bytes(), public_key_pem.into_bytes())
}

/// An example of a BGZF EOF data uri.
pub fn expected_bgzf_eof_data_url() -> String {
  "data:;base64,H4sIBAAAAAAA/wYAQkMCABsAAwAAAAAAAAAAAA==".to_string()
}

/// An example of a CRAM EOF data uri.
pub fn expected_cram_eof_data_url() -> String {
  "data:;base64,DwAAAP////8P4EVPRgAAAAABAAW92U8AAQAGBgEAAQABAO5jAUs=".to_string()
}

/// Get the default directory.
pub fn default_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .unwrap()
    .to_path_buf()
}

/// Get the default directory where data is present.
pub fn default_dir_data() -> PathBuf {
  default_dir().join("data")
}
