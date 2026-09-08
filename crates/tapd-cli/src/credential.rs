use std::env;

use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use zeroize::Zeroizing;

pub const KEYCHAIN_SERVICE: &str = "tapd-cli";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialSource {
    Environment,
    SystemKeychain,
}

pub struct ResolvedCredential {
    pub token: Zeroizing<String>,
    pub source: CredentialSource,
}

impl std::fmt::Debug for ResolvedCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedCredential")
            .field("token", &"[REDACTED]")
            .field("source", &self.source)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CredentialStatus {
    pub environment_variable: String,
    pub environment_set: bool,
    pub keychain_service: &'static str,
    pub keychain_account: String,
    pub keychain_stored: bool,
    pub effective_source: Option<CredentialSource>,
}

pub fn resolve(
    profile: &str,
    token_env: &str,
    require_token: bool,
) -> Result<Option<ResolvedCredential>> {
    if let Some(token) = token_from_environment(token_env) {
        return Ok(Some(ResolvedCredential {
            token,
            source: CredentialSource::Environment,
        }));
    }

    if !require_token {
        return Ok(None);
    }

    if let Some(token) = token_from_keychain(profile)? {
        return Ok(Some(ResolvedCredential {
            token,
            source: CredentialSource::SystemKeychain,
        }));
    }

    bail!(
        "profile {profile:?} has no credential; run `tapd-cli --profile {profile} auth login` or set {token_env}"
    )
}

pub fn status(profile: &str, token_env: &str) -> Result<CredentialStatus> {
    let environment_set = token_from_environment(token_env).is_some();
    let keychain_stored = token_from_keychain(profile)?.is_some();
    Ok(CredentialStatus {
        environment_variable: token_env.to_owned(),
        environment_set,
        keychain_service: KEYCHAIN_SERVICE,
        keychain_account: profile.to_owned(),
        keychain_stored,
        effective_source: effective_source(environment_set, keychain_stored),
    })
}

pub fn token_from_environment(token_env: &str) -> Option<Zeroizing<String>> {
    env::var(token_env)
        .ok()
        .filter(|value| !value.is_empty())
        .map(Zeroizing::new)
}

pub fn save(profile: &str, token: &str) -> Result<()> {
    if token.is_empty() {
        bail!("TAPD access token cannot be empty");
    }
    entry(profile)?
        .set_password(token)
        .map_err(|error| credential_error("save", profile, &error))
}

pub fn delete(profile: &str) -> Result<bool> {
    match entry(profile)?.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(error) => Err(credential_error("delete", profile, &error)),
    }
}

fn token_from_keychain(profile: &str) -> Result<Option<Zeroizing<String>>> {
    match entry(profile)?.get_password() {
        Ok(token) if token.is_empty() => Ok(None),
        Ok(token) => Ok(Some(Zeroizing::new(token))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(credential_error("read", profile, &error)),
    }
}

fn entry(profile: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, profile)
        .map_err(|error| credential_error("open", profile, &error))
}

fn credential_error(action: &str, profile: &str, error: &keyring::Error) -> anyhow::Error {
    anyhow!("failed to {action} system credential for profile {profile:?}: {error}")
}

const fn effective_source(
    environment_set: bool,
    keychain_stored: bool,
) -> Option<CredentialSource> {
    if environment_set {
        Some(CredentialSource::Environment)
    } else if keychain_stored {
        Some(CredentialSource::SystemKeychain)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{CredentialSource, effective_source};

    #[test]
    fn environment_has_precedence_over_keychain() {
        assert_eq!(
            effective_source(true, true),
            Some(CredentialSource::Environment)
        );
        assert_eq!(
            effective_source(false, true),
            Some(CredentialSource::SystemKeychain)
        );
        assert_eq!(effective_source(false, false), None);
    }
}
