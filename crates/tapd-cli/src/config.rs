use std::{collections::BTreeMap, env, fs, path::PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::credential::{self, CredentialSource};

pub const EXAMPLE_CONFIG: &str = include_str!("../../../examples/tapd-cli.example.toml");

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FileConfig {
    pub default_profile: Option<String>,
    pub profiles: BTreeMap<String, ProfileConfig>,
    pub organizations: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProfileConfig {
    pub api_base_url: String,
    pub web_base_url: String,
    pub workspace_id: Option<String>,
    pub token_env: String,
    pub timeout_seconds: u64,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            api_base_url: "https://api.tapd.cn".into(),
            web_base_url: "https://www.tapd.cn".into(),
            workspace_id: None,
            token_env: "TAPD_ACCESS_TOKEN".into(),
            timeout_seconds: 30,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub path: PathBuf,
    pub file: FileConfig,
}

#[derive(Clone)]
pub struct ResolvedProfile {
    pub name: String,
    pub config: ProfileConfig,
    pub workspace_id: Option<String>,
    pub access_token: Option<Zeroizing<String>>,
    pub credential_source: Option<CredentialSource>,
}

impl std::fmt::Debug for ResolvedProfile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedProfile")
            .field("name", &self.name)
            .field("config", &self.config)
            .field("workspace_id", &self.workspace_id)
            .field("credential_available", &self.access_token.is_some())
            .field("credential_source", &self.credential_source)
            .finish()
    }
}

impl LoadedConfig {
    pub fn load(explicit: Option<&PathBuf>) -> Result<Self> {
        let path = discover_path(explicit);
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        let file: FileConfig = toml::from_str(&content)
            .with_context(|| format!("failed to parse config file {}", path.display()))?;
        Ok(Self { path, file })
    }

    pub fn resolve_profile(
        &self,
        requested: Option<&str>,
        domain_default: Option<&str>,
        workspace_override: Option<&str>,
        require_token: bool,
    ) -> Result<ResolvedProfile> {
        let name = requested
            .or(domain_default)
            .or(self.file.default_profile.as_deref())
            .map(str::to_owned)
            .or_else(|| {
                (self.file.profiles.len() == 1)
                    .then(|| self.file.profiles.keys().next().cloned())
                    .flatten()
            })
            .ok_or_else(|| anyhow!("no profile selected; use --profile or default_profile"))?;
        let config = self
            .file
            .profiles
            .get(&name)
            .cloned()
            .ok_or_else(|| anyhow!("profile {name:?} does not exist"))?;
        let workspace_id = workspace_override
            .map(str::to_owned)
            .or_else(|| config.workspace_id.clone());
        let credential = credential::resolve(&name, &config.token_env, require_token)?;
        let credential_source = credential.as_ref().map(|value| value.source);
        let access_token = credential.map(|value| value.token);
        Ok(ResolvedProfile {
            name,
            config,
            workspace_id,
            access_token,
            credential_source,
        })
    }
}

pub fn discover_path(explicit: Option<&PathBuf>) -> PathBuf {
    if let Some(path) = explicit {
        return path.clone();
    }
    let local = PathBuf::from(".tapd-cli.toml");
    if local.exists() {
        return local;
    }
    default_config_path()
}

pub fn default_config_path() -> PathBuf {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(path).join("tapd-cli/config.toml");
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/tapd-cli/config.toml")
}

pub fn write_example(path: PathBuf, force: bool) -> Result<PathBuf> {
    if path.exists() && !force {
        bail!(
            "config file {} already exists; pass --force to replace this file",
            path.display()
        );
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&path, EXAMPLE_CONFIG)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}
