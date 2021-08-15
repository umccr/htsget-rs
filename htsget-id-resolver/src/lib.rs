mod regex_resolver;
pub use regex_resolver::RegexResolver;

pub trait HtsgetIdResolver {
  fn resolve_id(&self, id: &str) -> Option<String>;
}
