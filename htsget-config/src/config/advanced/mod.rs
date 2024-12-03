//! Advanced configuration.
//!

pub mod allow_guard;
pub mod cors;
pub mod file;
pub mod regex_location;
#[cfg(feature = "s3-storage")]
pub mod s3;
#[cfg(feature = "url-storage")]
pub mod url;
