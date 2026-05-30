use std::fs;
use std::io::Write;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::provider::{Kind, auth};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub provider: String,
    pub anthropic_model: String,
    pub openai_model: String,
    #[serde(default = "default_codex_model")]
    pub codex_model: String,
}

impl Config {
    pub fn kind(&self) -> Option<Kind> {
        Kind::from_str(&self.provider)
    }

    pub fn model_for(&self, kind: Kind) -> String {
        match kind {
            Kind::Anthropic => self.anthropic_model.clone(),
            Kind::OpenAi => self.openai_model.clone(),
            Kind::Codex => self.codex_model.clone(),
        }
    }
}

fn default_codex_model() -> String {
    Kind::Codex.default_model().to_string()
}

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("dev", "sanglee", "plz")
        .ok_or_else(|| Error::Config("could not determine config directory".into()))
}

pub fn config_dir() -> Result<PathBuf> {
    Ok(project_dirs()?.config_dir().to_path_buf())
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn load_or_init() -> Result<Config> {
    let path = config_path()?;
    if let Ok(text) = fs::read_to_string(&path) {
        let cfg: Config = toml::from_str(&text)
            .map_err(|e| Error::Config(format!("parsing {}: {e}", path.display())))?;
        return Ok(cfg);
    }

    let detected = autodetect_kind().ok_or(Error::NoApiKey)?;

    let cfg = Config {
        provider: detected.as_str().to_string(),
        anthropic_model: Kind::Anthropic.default_model().to_string(),
        openai_model: Kind::OpenAi.default_model().to_string(),
        codex_model: Kind::Codex.default_model().to_string(),
    };
    save(&cfg)?;
    Ok(cfg)
}

/// Prefer Anthropic when both providers are provisioned — picked arbitrarily;
/// users can override per-call with `--provider` or edit the config file.
/// "Provisioned" means either an env/stored API key or stored OAuth tokens.
fn autodetect_kind() -> Option<Kind> {
    if auth::has_any(Kind::Anthropic) {
        Some(Kind::Anthropic)
    } else if auth::has_any(Kind::OpenAi) {
        Some(Kind::OpenAi)
    } else if crate::provider::codex::has_auth() {
        Some(Kind::Codex)
    } else {
        None
    }
}

pub fn save(cfg: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(cfg)
        .map_err(|e| Error::Config(format!("serializing config: {e}")))?;
    let tmp = path.with_extension("toml.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, &path)?;
    Ok(())
}
