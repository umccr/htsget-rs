use lazy_static::lazy_static;
use regex::{Captures, Regex};

use crate::storage::StorageError::InvalidKey;

use super::Result;

/// Parses the given URL and determines if it is plausibly referring to an S3 object
/// and if so, returns the object bucket, key and potentially region.
/// Supports a variety of S3 formats
///   s3://bucket/key
///   https://bucket.s3.region.amazonaws.com/key
///   https://bucket.s3.amazonaws.com/key
///   https://s3.amazonaws.com/bucket/key
///   https://s3-region.amazonaws.com/bucket/key
///
pub fn parse_s3_url(s3_url: &str) -> Result<(String, String, Option<String>)> {
  // helpers to decompose captures into tuple
  fn extract_bucket_key(cap: Captures) -> (String, String, Option<String>) {
    (
      String::from(cap.name("bucket").unwrap().as_str()),
      String::from(cap.name("key").unwrap().as_str()),
      None,
    )
  }

  fn extract_bucket_key_region(cap: Captures) -> (String, String, Option<String>) {
    (
      String::from(cap.name("bucket").unwrap().as_str()),
      String::from(cap.name("key").unwrap().as_str()),
      Some(String::from(cap.name("region").unwrap().as_str())),
    )
  }

  // this is our S3 uri format - which is not officially a URI format but which is supported
  // by most S3 tools including the AWS cli tools
  let s3_capture = S3_URI_REGEX.captures(s3_url);

  if s3_capture.is_some() {
    return Ok(extract_bucket_key(s3_capture.unwrap()));
  }

  // this is the now AWS preferred format - which is virtual hosted style requests
  let virt_reg_capture = S3_VIRTUAL_STYLE_REGIONAL_URI_REGEX.captures(s3_url);

  if virt_reg_capture.is_some() {
    return Ok(extract_bucket_key_region(virt_reg_capture.unwrap()));
  }

  let virt_glob_capture = S3_VIRTUAL_STYLE_GLOBAL_URI_REGEX.captures(s3_url);

  if virt_glob_capture.is_some() {
    return Ok(extract_bucket_key(virt_glob_capture.unwrap()));
  }

  let path_reg_capture = S3_PATH_STYLE_REGIONAL_URI_REGEX.captures(s3_url);

  if path_reg_capture.is_some() {
    return Ok(extract_bucket_key_region(path_reg_capture.unwrap()));
  }

  let path_glob_capture = S3_PATH_STYLE_GLOBAL_URI_REGEX.captures(s3_url);

  if path_glob_capture.is_some() {
    return Ok(extract_bucket_key(path_glob_capture.unwrap()));
  }

  // TODO: get this to disable when not in local testing mode, or delete
  if s3_url.starts_with("http://") {
    // useful for local testing, not for prod
    let re = Regex::new(r"http://([^/]+)/(.*)").unwrap();
    let cap = re.captures(&s3_url).unwrap();
    let bucket = cap[1].to_string();
    let key = cap[2].to_string();

    return Ok((bucket, key, None));
  }

  Err(InvalidKey(s3_url.parse().unwrap()))
}

// ✓ Bucket names must be between 3 and 63 characters long.
// ✓ Bucket names can consist only of lowercase letters, numbers, dots (.), and hyphens (-).
// ✓ Bucket names must begin and end with a letter or number.
static BUCKET_PART: &'static str = r##"(?P<bucket>[a-z0-9][a-z0-9-\.]{1,61}[a-z0-9])"##;

static KEY_PART: &'static str = r##"(?P<key>.+)"##;

// this regex is not super specific (i.e doesn't match match east/west etc) but does at least capture the 'vibe' of AWS regions
static REGION_PART: &'static str =
  r##"(?P<region>(us(-gov)?|af|ap|ca|cn|eu|me|sa)-[a-z]{1,16}-\d)"##;

lazy_static! {
    // uri example 's3://my_bucket/foobar/file.mp3'
    static ref S3_URI_REGEX: Regex =
        Regex::new(format!("^s3://{}/{}$", BUCKET_PART, KEY_PART).as_str()).unwrap();

    // Amazon S3 virtual hosted style URLs follow the format shown below.
    //   https://bucket-name.s3.Region.amazonaws.com/key name
    static ref S3_VIRTUAL_STYLE_GLOBAL_URI_REGEX: Regex =
        Regex::new(format!("^https://{}\\.s3\\.amazonaws\\.com/{}$", BUCKET_PART, KEY_PART).as_str()).unwrap();
    static ref S3_VIRTUAL_STYLE_REGIONAL_URI_REGEX: Regex =
        Regex::new(format!("^https://{}\\.s3\\.{}\\.amazonaws\\.com/{}$", BUCKET_PART, REGION_PART, KEY_PART).as_str()).unwrap();

    // old style paths that we will recognise even though they are being deprecated
    // (just because we recognise them in this format doesn't mean we are actually using this format to access them)
    static ref S3_PATH_STYLE_GLOBAL_URI_REGEX: Regex =
        Regex::new(format!("^https://s3\\.amazonaws\\.com/{}/{}$", BUCKET_PART, KEY_PART).as_str()).unwrap();
    static ref S3_PATH_STYLE_REGIONAL_URI_REGEX: Regex =
        Regex::new(format!("^https://s3-{}\\.amazonaws\\.com/{}/{}$", REGION_PART, BUCKET_PART, KEY_PART).as_str()).unwrap();
}

#[cfg(test)]
mod tests {
  use super::*;

  fn assert_not_match(uri: &str) {
    let result = parse_s3_url(uri);

    assert!(
      result.is_err(),
      "Uri was not expected to be recognised as valid S3 but it was"
    );
  }

  fn assert_match(uri: &str, bucket: &str, key: &str, region: Option<&str>) {
    let result = parse_s3_url(uri);

    assert!(
      result.is_ok(),
      "Uri was expected to be recognised as valid S3 but was not"
    );

    let result_actual = result.unwrap();

    assert_eq!(result_actual.0, bucket);
    assert_eq!(result_actual.1, key);

    if region.is_some() {
      assert_eq!(result_actual.2.unwrap(), region.unwrap());
    }
  }

  #[test]
  fn uri_style() {
    assert_match(
      "s3://jbarr-public/images/abc.jpeg",
      "jbarr-public",
      "images/abc.jpeg",
      None,
    );
  }

  #[test]
  fn path_style() {
    assert_match(
      "https://s3-us-east-2.amazonaws.com/jbarr-public/images/abc.jpeg",
      "jbarr-public",
      "images/abc.jpeg",
      Some("us-east-2"),
    );
    assert_match(
      "https://s3.amazonaws.com/jbarr-public/images/abc.jpeg",
      "jbarr-public",
      "images/abc.jpeg",
      None,
    );
  }

  #[test]
  fn virtual_style() {
    assert_match(
      "https://jbarr-public.s3.us-east-2.amazonaws.com/images/abc.jpeg",
      "jbarr-public",
      "images/abc.jpeg",
      Some("us-east-2"),
    );
    assert_match(
      "https://jbarr-public.s3.amazonaws.com/images/abc.jpeg",
      "jbarr-public",
      "images/abc.jpeg",
      None,
    );
  }

  #[test]
  fn virtual_style_bucket_like_region() {
    // no reason a bucket can't have a name that looks like a region
    assert_match(
      "https://ap-southeast-2.s3.us-east-2.amazonaws.com/images/abc.jpeg",
      "ap-southeast-2",
      "images/abc.jpeg",
      Some("us-east-2"),
    );
  }

  #[test]
  fn path_style_invalid() {
    // the character x here ..s3xamazon.. is to test out that our '.' in the paths are not being
    // matched as regex wildcards
    assert_not_match("https://s3xamazonaws.com/jbarr-public/images/abc.jpeg");
  }

  #[test]
  fn virtual_style_invalid() {
    // not long enough bucket name (only 2 characters)
    assert_not_match("https://to.s3.ap-southeast-2/images/abc.jpeg");
    // bucket names can't end with .
    assert_not_match("https://abucketname..s3.ap-southeast-2/images/abc.jpeg");
    // _ is not valid in a bucket name
    assert_not_match("https://bucket_name.s3.ap-southeast-2/images/abc.jpeg");
  }
}
