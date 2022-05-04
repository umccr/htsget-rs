use regex::{Error, Regex};
use serde::Deserialize;

pub trait HtsGetIdResolver {
  fn resolve_id(&self, id: &str) -> Option<String>;
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RegexResolver {
  #[serde(with = "serde_regex")]
  pub(crate) regex: Regex,
  pub(crate) substitution_string: String,
}

impl Default for RegexResolver {
  fn default() -> Self {
    Self::new(".*", "$0").expect("Expected valid resolver.")
  }
}

impl RegexResolver {
  pub fn new(regex: &str, replacement_string: &str) -> Result<Self, Error> {
    Ok(RegexResolver {
      regex: Regex::new(regex)?,
      substitution_string: replacement_string.to_string(),
    })
  }
}

impl HtsGetIdResolver for RegexResolver {
  fn resolve_id(&self, id: &str) -> Option<String> {
    if self.regex.is_match(id) {
      Some(
        self
          .regex
          .replace(id, &self.substitution_string)
          .to_string(),
      )
    } else {
      None
    }
  }
}

#[cfg(test)]
pub mod tests {
  use super::*;

  #[test]
  fn resolver_resolve_id() {
    let resolver = RegexResolver::new(".*", "$0-test").unwrap();
    assert_eq!(resolver.resolve_id("id").unwrap(), "id-test");
  }
}
