//! Authorization source config.
//!

use crate::config::advanced::auth::AuthorizationRestrictions;
use crate::config::advanced::callout::Callout;
use crate::error::Error::ParseError;
use crate::error::Result;
use serde::Deserialize;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

/// Where authorization restrictions come from, either the remote server or a static file.
#[derive(Debug, Clone)]
pub enum AuthorizationSource {
  Callout(Callout),
  Static(AuthorizationRestrictions),
}

impl AuthorizationSource {
  /// Get the callout if the type is a `Callout`.
  pub fn callout(&self) -> Option<&Callout> {
    match self {
      Self::Callout(callout) => Some(callout),
      Self::Static(_) => None,
    }
  }

  /// Get a mutable reference to the callout, if the type is a `Callout`.
  pub fn callout_mut(&mut self) -> Option<&mut Callout> {
    match self {
      Self::Callout(callout) => Some(callout),
      Self::Static(_) => None,
    }
  }

  /// Get the static restrictions if the type is `Static`.
  pub fn static_restrictions(&self) -> Option<&AuthorizationRestrictions> {
    match self {
      Self::Static(restrictions) => Some(restrictions),
      Self::Callout(_) => None,
    }
  }
}

/// Builder for `AuthorizationSource`.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthorizationSourceBuilder {
  Callout(Callout),
  Static { path: PathBuf },
}

impl AuthorizationSourceBuilder {
  /// Build an `AuthorizationSource`.
  pub fn build(self) -> Result<AuthorizationSource> {
    match self {
      Self::Callout(callout) => Ok(AuthorizationSource::Callout(callout)),
      Self::Static { path } => {
        let mut buf = vec![];
        File::open(&path)?.read_to_end(&mut buf)?;

        let restrictions: AuthorizationRestrictions = serde_json::from_slice(&buf)
          .map_err(|err| ParseError(format!("parsing {}: {err}", path.display())))?;

        Ok(AuthorizationSource::Static(restrictions))
      }
    }
  }
}
