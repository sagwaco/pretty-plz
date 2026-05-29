//! Storage for user-pasted API keys.
//!
//! Keys live at `<config_dir>/keys/<provider>.txt`, mode 0600. Written by
//! `plz login`, removed by `plz logout`. An alternative to the
//! `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` environment variables — the env vars
//! always win in `--auth auto` mode, with stored keys as the fallback before
//! OAuth.

use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;

use crate::error::{Error, Result};
use crate::provider::Kind;
use crate::secret_file;

fn keys_dir() -> Result<PathBuf> {
    let pd = ProjectDirs::from("dev", "sanglee", "plz")
        .ok_or_else(|| Error::Config("could not determine config directory".into()))?;
    Ok(pd.config_dir().join("keys"))
}

pub fn path_for(kind: Kind) -> Result<PathBuf> {
    Ok(keys_dir()?.join(format!("{}.txt", kind.as_str())))
}

/// Read the stored API key, trimmed. Returns Ok(None) when the file doesn't
/// exist or contains only whitespace.
pub fn load(kind: Kind) -> Result<Option<String>> {
    let path = path_for(kind)?;
    match fs::read_to_string(&path) {
        Ok(s) => {
            let trimmed = s.trim();
            Ok(if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::Io(e)),
    }
}

pub fn save(kind: Kind, key: &str) -> Result<()> {
    let path = path_for(kind)?;
    secret_file::save(&path, key.trim().as_bytes())
}

pub fn delete(kind: Kind) -> Result<()> {
    let path = path_for(kind)?;
    secret_file::delete(&path)
}
