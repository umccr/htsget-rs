//! HTTP-callout config which is shared across auth and backend url storage types.
//!

use crate::config::advanced::HttpClient;
use crate::error::Error::ParseError;
use crate::error::{Error, Result};
use crate::http::client::HttpClientConfig;
use heck::ToTrainCase;
use http::{HeaderMap, Uri};
use serde::{Deserialize, Serialize};
use wildmatch::WildMatch;

/// A callout to a remote server.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Callout {
  #[serde(with = "http_serde::uri")]
  url: Uri,
  #[serde(default = "default_http_client", alias = "tls")]
  http: HttpClient,
  #[serde(default)]
  forward: Forward,
}

fn default_http_client() -> HttpClient {
  HttpClient::from(HttpClientConfig::default())
}

impl Callout {
  /// Create a new callout.
  pub fn new(url: Uri, http: HttpClient, forward: Forward) -> Self {
    Self { url, http, forward }
  }

  /// The callout URL.
  pub fn url(&self) -> &Uri {
    &self.url
  }

  /// The HTTP client.
  pub fn http(&self) -> &HttpClient {
    &self.http
  }

  /// Mutable HTTP client.
  pub fn http_mut(&mut self) -> &mut HttpClient {
    &mut self.http
  }

  /// What data to forward to the callout server.
  pub fn forward(&self) -> &Forward {
    &self.forward
  }
}

/// Forward data from the client request to the callout server. This includes
/// headers from the client, and htsget-specific context, i.e. endpoint, id,
/// extensions, etc.
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields, default)]
pub struct Forward {
  headers: HeaderRules,
  context: ContextRules,
}

impl Forward {
  /// Create a forward config.
  pub fn new(headers: HeaderRules, context: ContextRules) -> Self {
    Self { headers, context }
  }

  /// Header rules.
  pub fn headers(&self) -> &HeaderRules {
    &self.headers
  }

  /// Context rules.
  pub fn context(&self) -> &ContextRules {
    &self.context
  }
}

/// Allow and deny rules for header names. Both lists support `*` and `?`.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct HeaderRules {
  allow: Vec<String>,
  deny: Vec<String>,
}

impl HeaderRules {
  /// Create new header rules.
  pub fn new(allow: Vec<String>, deny: Vec<String>) -> Self {
    Self { allow, deny }
  }

  /// Allow patterns.
  pub fn allow(&self) -> &[String] {
    &self.allow
  }

  /// Deny patterns.
  pub fn deny(&self) -> &[String] {
    &self.deny
  }

  /// Filter a header map, keeping only headers whose names match at least one of the allow
  /// patterns and do not match any of the deny patterns.
  pub fn filter(&self, headers: &HeaderMap) -> HeaderMap {
    let allow: Vec<_> = self
      .allow
      .iter()
      .map(|p| WildMatch::new(&p.to_lowercase()))
      .collect();
    let deny: Vec<_> = self
      .deny
      .iter()
      .map(|p| WildMatch::new(&p.to_lowercase()))
      .collect();

    if allow.is_empty() {
      return HeaderMap::new();
    }

    let mut result = HeaderMap::new();
    for (name, value) in headers {
      let lowered = name.as_str().to_lowercase();
      if allow.iter().any(|p| p.matches(&lowered))
        && !deny.iter().any(|p| p.matches(&lowered))
      {
        result.insert(name, value.clone());
      }
    }
    result
  }
}

/// The htsget-specific header values to insert into the callout request.
///
/// These values are derived from the kind of request to htsget, like the endpoint and
/// id. Headers are inserted with a `Htsget-Context-` prefix.
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields, default)]
pub struct ContextRules {
  endpoint_type: bool,
  id: bool,
  extensions: Vec<ContextExtension>,
}

impl ContextRules {
  /// Create new context rules.
  pub fn new(endpoint_type: bool, id: bool, extensions: Vec<ContextExtension>) -> Self {
    Self {
      endpoint_type,
      id,
      extensions,
    }
  }

  /// Whether to forward the endpoint type as a context header.
  pub fn endpoint_type(&self) -> bool {
    self.endpoint_type
  }

  /// Whether to forward the request id as a context header.
  pub fn id(&self) -> bool {
    self.id
  }

  /// JSONPath extension headers.
  pub fn extensions(&self) -> &[ContextExtension] {
    &self.extensions
  }
}

/// A header derived by using JSONPath on the request's extension, e.g. from Lambda contexts
/// or other axum extensions.
///
/// `name` is optional. When omitted, it is derived from the JSONPath by applying case conversion
/// on the components. E.g. `$.user.custom_id` becomes `Htsget-Context-User-Custom-Id`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields, try_from = "ContextExtensionRaw")]
pub struct ContextExtension {
  json_path: String,
  name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextExtensionRaw {
  json_path: String,
  #[serde(default)]
  name: Option<String>,
}

impl ContextExtensionRaw {
  fn derive_name(json_path: &str) -> Result<String> {
    let derived = json_path.to_train_case();

    if derived.is_empty() {
      return Err(ParseError(format!(
        "cannot derive header name from JSONPath `{json_path}`, specify `name` explicitly"
      )));
    }

    Ok(derived)
  }
}

impl TryFrom<ContextExtensionRaw> for ContextExtension {
  type Error = Error;

  fn try_from(raw: ContextExtensionRaw) -> Result<Self> {
    let name = match raw.name {
      Some(name) => name,
      None => ContextExtensionRaw::derive_name(&raw.json_path)?,
    };
    Ok(Self {
      json_path: raw.json_path,
      name,
    })
  }
}

impl ContextExtension {
  /// Create a new context extension.
  pub fn new(json_path: String, name: String) -> Self {
    Self { json_path, name }
  }

  /// The JSONPath expression.
  pub fn json_path(&self) -> &str {
    &self.json_path
  }

  /// The header name.
  pub fn name(&self) -> &str {
    &self.name
  }
}

/// How to interpret a fetched object.
#[derive(Deserialize, Debug, Clone)]
#[serde(try_from = "ParseRaw")]
pub enum Parse {
  /// The incoming object is raw bytes, with the option to override the ticket URL.
  Bytes { ticket_url: Option<Uri> },
  /// The incoming object is JSON, where JSONPath specifies how to find the data and location.
  JsonPath {
    content_path: String,
    size_path: Option<String>,
    ticket: Option<TicketSource>,
  },
}

/// Where the URL tickets come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TicketSource {
  /// Take the URL ticket from a JSONPath.
  JsonPath { path: String },
  /// Use a static URL for tickets.
  Url {
    url: Uri,
  },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ParseRaw {
  Bytes {
    #[serde(default, with = "http_serde::option::uri")]
    ticket_url: Option<Uri>,
  },
  JsonPath {
    content_path: String,
    #[serde(default)]
    size_path: Option<String>,
    #[serde(default)]
    ticket_path: Option<String>,
    #[serde(default, with = "http_serde::option::uri")]
    ticket_url: Option<Uri>,
  },
}

impl TryFrom<ParseRaw> for Parse {
  type Error = Error;

  fn try_from(raw: ParseRaw) -> Result<Self> {
    match raw {
      ParseRaw::Bytes { ticket_url } => Ok(Parse::Bytes { ticket_url }),
      ParseRaw::JsonPath {
        content_path,
        size_path,
        ticket_path,
        ticket_url,
      } => {
        let ticket = match (ticket_path, ticket_url) {
          (None, None) => None,
          (None, Some(url)) => Some(TicketSource::Url { url }),
          (Some(path), None) => Some(TicketSource::JsonPath { path }),
          (Some(_), Some(_)) => return Err(ParseError(
            "cannot specify both `ticket_path` and `ticket_url`".to_string(),
          ))
        };

        Ok(Parse::JsonPath {
          content_path,
          size_path,
          ticket,
        })
      }
    }
  }
}


/// Which headers from the response to echo back to the client in the ticket.
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields, default)]
pub struct Reflect {
  headers: HeaderRules,
}

impl Reflect {
  /// Create a new reflect config.
  pub fn new(headers: HeaderRules) -> Self {
    Self { headers }
  }

  /// Header rules.
  pub fn headers(&self) -> &HeaderRules {
    &self.headers
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn callout_minimal() {
    let toml = r#"url = "https://example.com""#;
    let callout: Callout = toml::from_str(toml).unwrap();
    assert_eq!(callout.url().to_string(), "https://example.com/");
    assert!(callout.forward().headers().allow().is_empty());
    assert!(callout.forward().headers().deny().is_empty());
    assert!(!callout.forward().context().endpoint_type());
    assert!(!callout.forward().context().id());
    assert!(callout.forward().context().extensions().is_empty());
  }

  #[test]
  fn callout_complex() {
    let toml = r#"
      url = "https://example.com"

      [forward]
      headers.allow = ["Authorization", "X-Custom-*"]
      headers.deny  = ["X-Internal-*"]

      [forward.context]
      endpoint_type = true
      id            = true
      extensions    = [{ json_path = "$.custom", name = "Custom-Name" }]
    "#;
    let callout: Callout = toml::from_str(toml).unwrap();
    assert_eq!(
      callout.forward().headers().allow(),
      &["Authorization".to_string(), "X-Custom-*".to_string()]
    );
    assert_eq!(
      callout.forward().headers().deny(),
      &["X-Internal-*".to_string()]
    );
    assert!(callout.forward().context().endpoint_type());
    assert!(callout.forward().context().id());
    assert_eq!(
      callout.forward().context().extensions(),
      &[ContextExtension::new(
        "$.custom".to_string(),
        "Custom-Name".to_string()
      )]
    );
  }

  #[test]
  fn context_extension_derives_names() {
    let toml = r#"json_path = "$.custom_id""#;
    let ext: ContextExtension = toml::from_str(toml).unwrap();
    assert_eq!(ext.name(), "Custom-Id");

    let toml = r#"json_path = "$.user.custom_id""#;
    let ext: ContextExtension = toml::from_str(toml).unwrap();
    assert_eq!(ext.name(), "User-Custom-Id");

    let toml = r#"json_path = "$..custom""#;
    let ext: ContextExtension = toml::from_str(toml).unwrap();
    assert_eq!(ext.name(), "Custom");

    let toml = r#"json_path = "$.user.custom_id[0]""#;
    let ext: ContextExtension = toml::from_str(toml).unwrap();
    assert_eq!(ext.name(), "User-Custom-Id-0");

    let toml = r#"json_path = "$.""#;
    assert!(toml::from_str::<ContextExtension>(toml).is_err());
  }

  #[test]
  fn parse_bytes() {
    let toml = r#"kind = "bytes""#;
    let parse = toml::from_str(toml).unwrap();
    assert!(matches!(parse, Parse::Bytes { ticket_url: None }));

    let toml = r#"
      kind = "bytes"
      ticket_url = "https://example.com"
    "#;
    let parse = toml::from_str(toml).unwrap();
    match parse {
      Parse::Bytes { ticket_url } => {
        assert_eq!(ticket_url.unwrap().to_string(), "https://example.com/");
      }
      _ => panic!(),
    }
  }

  #[test]
  fn parse_json_path() {
    let toml = r#"
      kind = "json_path"
      content_path = "$.content"
      size_path    = "$.size"
      ticket_path  = "$.response"
    "#;
    let parse = toml::from_str(toml).unwrap();
    match parse {
      Parse::JsonPath {
        content_path,
        size_path,
        ticket,
      } => {
        assert_eq!(content_path, "$.content");
        assert_eq!(size_path.as_deref(), Some("$.size"));
        assert_eq!(
          ticket,
          Some(TicketSource::JsonPath { path: "$.response".to_string()} ),
        );
      }
      _ => panic!(),
    }

    let toml = r#"
      kind = "json_path"
      content_path = "$.content"
      ticket_url  = "https://example.com"
    "#;
    let parse = toml::from_str(toml).unwrap();
    match parse {
      Parse::JsonPath { ticket, .. } => {
        assert_eq!(
          ticket,
          Some(TicketSource::Url { url: "https://example.com".parse().unwrap()} ),
        );
      }
      _ => panic!(),
    }

    let toml = r#"
      kind = "json_path"
      content_path = "$.content"
    "#;
    let parse = toml::from_str(toml).unwrap();
    match parse {
      Parse::JsonPath { ticket, .. } => {
        assert!(
          ticket.is_none(),
        );
      }
      _ => panic!(),
    }

    let toml = r#"
      kind = "json_path"
      content_path = "$.content"
      ticket_path  = "$.response"
      ticket_url  = "https://example.com"
    "#;
    let parse = toml::from_str::<Parse>(toml);
    assert!(parse.is_err());
  }

  #[test]
  fn reflect_default() {
    let reflect: Reflect = toml::from_str("").unwrap();
    assert!(reflect.headers().allow().is_empty());
    assert!(reflect.headers().deny().is_empty());

    let toml = r#"
      headers.allow = ["Authorization"]
      headers.deny  = ["X-Custom-*"]
    "#;
    let reflect: Reflect = toml::from_str(toml).unwrap();
    assert_eq!(reflect.headers().allow(), &["Authorization".to_string()]);
    assert_eq!(reflect.headers().deny(), &["X-Custom-*".to_string()]);
  }
}
