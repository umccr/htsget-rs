use regex::{Error, Regex};
use serde::Deserialize;

pub trait HtsGetIdResolver {
  fn resolve_id(&self, id: &str) -> Option<String>;
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegexResolver {
  #[serde(with = "serde_regex")]
  regex: Regex,
  substitution_string: String,
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
