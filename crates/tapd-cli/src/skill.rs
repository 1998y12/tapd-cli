use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::cli::{SkillAgent, SkillScope, SkillTargetArgs};

const SKILL_NAME: &str = "tapd-cli";
const MANIFEST_NAME: &str = ".tapd-cli-skill.json";
const INSTALLER_NAME: &str = "tapd-cli";

const BUNDLE_FILES: &[EmbeddedFile] = &[
    EmbeddedFile {
        path: "SKILL.md",
        contents: include_str!("../../../skills/tapd-cli/SKILL.md"),
    },
    EmbeddedFile {
        path: "references/auth-and-config.md",
        contents: include_str!("../../../skills/tapd-cli/references/auth-and-config.md"),
    },
    EmbeddedFile {
        path: "references/workspaces-and-output.md",
        contents: include_str!("../../../skills/tapd-cli/references/workspaces-and-output.md"),
    },
    EmbeddedFile {
        path: "references/stories.md",
        contents: include_str!("../../../skills/tapd-cli/references/stories.md"),
    },
    EmbeddedFile {
        path: "references/comments.md",
        contents: include_str!("../../../skills/tapd-cli/references/comments.md"),
    },
    EmbeddedFile {
        path: "references/attachments-and-images.md",
        contents: include_str!("../../../skills/tapd-cli/references/attachments-and-images.md"),
    },
    EmbeddedFile {
        path: "references/workflows-and-metadata.md",
        contents: include_str!("../../../skills/tapd-cli/references/workflows-and-metadata.md"),
    },
    EmbeddedFile {
        path: "references/domain-extension.md",
        contents: include_str!("../../../skills/tapd-cli/references/domain-extension.md"),
    },
];

#[derive(Debug, Clone, Copy)]
struct EmbeddedFile {
    path: &'static str,
    contents: &'static str,
}

#[derive(Debug)]
pub enum SkillOutput {
    Json(Value),
    Plain(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstallManifest {
    skill: String,
    version: String,
    bundle_hash: String,
    installed_by: String,
    files: Vec<String>,
}

#[derive(Debug, Clone)]
struct InstallTarget {
    agent: String,
    scope: String,
    path: PathBuf,
}

pub fn list() -> SkillOutput {
    SkillOutput::Json(json!({
        "skill": SKILL_NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "bundle_hash": bundle_hash(),
        "files": BUNDLE_FILES.iter().map(|file| file.path).collect::<Vec<_>>(),
    }))
}

pub fn read(path: &str) -> Result<SkillOutput> {
    let path = path.trim_start_matches("./");
    let file = BUNDLE_FILES
        .iter()
        .find(|file| file.path == path)
        .ok_or_else(|| anyhow!("embedded Skill file {path:?} does not exist"))?;
    Ok(SkillOutput::Plain(file.contents.into()))
}

pub fn install(args: &SkillTargetArgs, force: bool) -> Result<SkillOutput> {
    let targets = targets(args, TargetMode::Install)?;
    let results = targets
        .iter()
        .map(|target| install_target(target, force, false))
        .collect::<Result<Vec<_>>>()?;
    Ok(SkillOutput::Json(json!({
        "skill": SKILL_NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "bundle_hash": bundle_hash(),
        "installed": results,
    })))
}

pub fn status(args: &SkillTargetArgs) -> Result<SkillOutput> {
    let targets = targets(args, TargetMode::Status)?;
    let statuses = targets
        .iter()
        .map(target_status)
        .collect::<Result<Vec<_>>>()?;
    Ok(SkillOutput::Json(json!({
        "skill": SKILL_NAME,
        "bundled_version": env!("CARGO_PKG_VERSION"),
        "bundle_hash": bundle_hash(),
        "targets": statuses,
    })))
}

pub fn update(args: &SkillTargetArgs, force: bool) -> Result<SkillOutput> {
    let targets = targets(args, TargetMode::ManagedMutation)?;
    let results = targets
        .iter()
        .map(|target| update_target(target, force))
        .collect::<Result<Vec<_>>>()?;
    Ok(SkillOutput::Json(json!({
        "skill": SKILL_NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "bundle_hash": bundle_hash(),
        "updated": results,
    })))
}

pub fn uninstall(args: &SkillTargetArgs) -> Result<SkillOutput> {
    let targets = targets(args, TargetMode::ManagedMutation)?;
    let results = targets
        .iter()
        .map(uninstall_target)
        .collect::<Result<Vec<_>>>()?;
    Ok(SkillOutput::Json(json!({
        "skill": SKILL_NAME,
        "uninstalled": results,
    })))
}

#[derive(Debug, Clone, Copy)]
enum TargetMode {
    Install,
    ManagedMutation,
    Status,
}

fn targets(args: &SkillTargetArgs, mode: TargetMode) -> Result<Vec<InstallTarget>> {
    if let Some(directory) = &args.dir {
        return Ok(vec![InstallTarget {
            agent: "custom".into(),
            scope: "custom".into(),
            path: directory.join(SKILL_NAME),
        }]);
    }

    if matches!(args.scope, SkillScope::Project) {
        let root = project_root()?;
        let path = root.join(".agents/skills").join(SKILL_NAME);
        return Ok(vec![InstallTarget {
            agent: if matches!(args.agent, SkillAgent::Auto) {
                "shared-project".into()
            } else {
                agent_name(args.agent).into()
            },
            scope: "project".into(),
            path,
        }]);
    }

    let agents = match args.agent {
        SkillAgent::Auto => {
            let detected = [
                SkillAgent::Codex,
                SkillAgent::ClaudeCode,
                SkillAgent::Cursor,
            ]
            .into_iter()
            .filter(|agent| {
                let Some(root) = agent_config_root(*agent) else {
                    return false;
                };
                match mode {
                    TargetMode::Status => true,
                    TargetMode::Install => root.exists(),
                    TargetMode::ManagedMutation => root
                        .join("skills")
                        .join(SKILL_NAME)
                        .join(MANIFEST_NAME)
                        .is_file(),
                }
            })
            .collect::<Vec<_>>();
            if detected.is_empty() {
                let message = match mode {
                    TargetMode::Install => {
                        "no supported agent installation was detected; pass --agent or --dir explicitly"
                    }
                    TargetMode::ManagedMutation => {
                        "no managed tapd-cli Skill installation was detected; pass --agent or --dir explicitly"
                    }
                    TargetMode::Status => unreachable!("status always returns known adapters"),
                };
                bail!(message);
            }
            detected
        }
        agent => vec![agent],
    };
    agents
        .into_iter()
        .map(|agent| {
            let config_root = agent_config_root(agent).ok_or_else(|| {
                anyhow!(
                    "could not determine the home directory for {}",
                    agent_name(agent)
                )
            })?;
            Ok(InstallTarget {
                agent: agent_name(agent).into(),
                scope: "user".into(),
                path: config_root.join("skills").join(SKILL_NAME),
            })
        })
        .collect()
}

fn agent_config_root(agent: SkillAgent) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match agent {
        SkillAgent::Codex => {
            Some(env::var_os("CODEX_HOME").map_or_else(|| home.join(".codex"), PathBuf::from))
        }
        SkillAgent::ClaudeCode => Some(
            env::var_os("CLAUDE_CONFIG_DIR").map_or_else(|| home.join(".claude"), PathBuf::from),
        ),
        SkillAgent::Cursor => Some(home.join(".cursor")),
        SkillAgent::Auto => None,
    }
}

const fn agent_name(agent: SkillAgent) -> &'static str {
    match agent {
        SkillAgent::Auto => "auto",
        SkillAgent::Codex => "codex",
        SkillAgent::ClaudeCode => "claude-code",
        SkillAgent::Cursor => "cursor",
    }
}

fn project_root() -> Result<PathBuf> {
    let mut current = env::current_dir().context("failed to read the current directory")?;
    loop {
        if current.join(".git").exists() {
            return Ok(current);
        }
        if !current.pop() {
            bail!("project scope requires running inside a Git repository");
        }
    }
}

fn install_target(target: &InstallTarget, force: bool, update: bool) -> Result<Value> {
    if update {
        require_managed_installation(target)?;
    } else if target.path.exists() && !force {
        bail!(
            "Skill destination {} already exists; use skill update for a managed installation or --force to overwrite known files",
            target.path.display()
        );
    }

    fs::create_dir_all(target.path.join("references"))
        .with_context(|| format!("failed to create {}", target.path.display()))?;
    for file in BUNDLE_FILES {
        let destination = target.path.join(file.path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&destination, file.contents)
            .with_context(|| format!("failed to write {}", destination.display()))?;
    }
    let manifest = InstallManifest {
        skill: SKILL_NAME.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        bundle_hash: bundle_hash(),
        installed_by: INSTALLER_NAME.into(),
        files: BUNDLE_FILES
            .iter()
            .map(|file| file.path.to_owned())
            .collect(),
    };
    let manifest_path = target.path.join(MANIFEST_NAME);
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    Ok(json!({
        "agent": target.agent,
        "scope": target.scope,
        "path": target.path,
        "files_written": BUNDLE_FILES.len() + 1,
    }))
}

fn update_target(target: &InstallTarget, force: bool) -> Result<Value> {
    let manifest = require_managed_installation(target)?;
    let installed_hash = installed_bundle_hash(target, &manifest)?;
    if !force && installed_hash.as_deref() != Some(manifest.bundle_hash.as_str()) {
        bail!(
            "managed Skill files at {} were modified or are incomplete; inspect them and pass --force to replace known files",
            target.path.display()
        );
    }
    install_target(target, true, true)
}

fn target_status(target: &InstallTarget) -> Result<Value> {
    let manifest_path = target.path.join(MANIFEST_NAME);
    if !manifest_path.exists() {
        return Ok(json!({
            "agent": target.agent,
            "scope": target.scope,
            "path": target.path,
            "installed": target.path.join("SKILL.md").exists(),
            "managed": false,
            "installed_version": null,
            "state": if target.path.join("SKILL.md").exists() { "unmanaged" } else { "missing" },
        }));
    }
    let manifest = read_manifest(target)?;
    let current_hash = bundle_hash();
    let installed_hash = installed_bundle_hash(target, &manifest)?;
    let state = if installed_hash.is_none() {
        "incomplete"
    } else if installed_hash.as_deref() != Some(manifest.bundle_hash.as_str()) {
        "modified"
    } else if manifest.bundle_hash == current_hash {
        "current"
    } else {
        "update_available"
    };
    Ok(json!({
        "agent": target.agent,
        "scope": target.scope,
        "path": target.path,
        "installed": true,
        "managed": manifest.installed_by == INSTALLER_NAME,
        "installed_version": manifest.version,
        "installed_bundle_hash": manifest.bundle_hash,
        "actual_bundle_hash": installed_hash,
        "state": state,
    }))
}

fn uninstall_target(target: &InstallTarget) -> Result<Value> {
    let manifest = require_managed_installation(target)?;
    for relative in &manifest.files {
        if !BUNDLE_FILES.iter().any(|file| file.path == relative) {
            bail!(
                "managed manifest at {} contains unexpected file {relative:?}",
                target.path.display()
            );
        }
    }
    for relative in &manifest.files {
        let path = target.path.join(relative);
        if path.is_file() {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
    }
    let manifest_path = target.path.join(MANIFEST_NAME);
    if manifest_path.is_file() {
        fs::remove_file(&manifest_path)
            .with_context(|| format!("failed to remove {}", manifest_path.display()))?;
    }
    let references = target.path.join("references");
    if references.is_dir() {
        let _ = fs::remove_dir(&references);
    }
    if target.path.is_dir() {
        let _ = fs::remove_dir(&target.path);
    }

    Ok(json!({
        "agent": target.agent,
        "scope": target.scope,
        "path": target.path,
        "removed_managed_files": manifest.files.len() + 1,
        "destination_remaining": target.path.exists(),
    }))
}

fn require_managed_installation(target: &InstallTarget) -> Result<InstallManifest> {
    let manifest = read_manifest(target)?;
    if manifest.skill != SKILL_NAME || manifest.installed_by != INSTALLER_NAME {
        bail!(
            "refusing to modify unmanaged Skill destination {}",
            target.path.display()
        );
    }
    validate_manifest_files(target, &manifest)?;
    Ok(manifest)
}

fn validate_manifest_files(target: &InstallTarget, manifest: &InstallManifest) -> Result<()> {
    let expected = BUNDLE_FILES
        .iter()
        .map(|file| file.path.to_owned())
        .collect::<Vec<_>>();
    if manifest.files != expected {
        bail!(
            "managed manifest at {} contains an unexpected file list",
            target.path.display()
        );
    }
    Ok(())
}

fn installed_bundle_hash(
    target: &InstallTarget,
    manifest: &InstallManifest,
) -> Result<Option<String>> {
    validate_manifest_files(target, manifest)?;
    let mut digest = Sha256::new();
    for relative in &manifest.files {
        let path = target.path.join(relative);
        if !path.is_file() {
            return Ok(None);
        }
        let contents = fs::read(&path)
            .with_context(|| format!("failed to read installed Skill file {}", path.display()))?;
        digest.update(relative.as_bytes());
        digest.update([0]);
        digest.update(contents);
        digest.update([0]);
    }
    Ok(Some(hex::encode(digest.finalize())))
}

fn read_manifest(target: &InstallTarget) -> Result<InstallManifest> {
    let path = target.path.join(MANIFEST_NAME);
    let bytes = fs::read(&path)
        .with_context(|| format!("managed Skill manifest {} was not found", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse managed Skill manifest {}", path.display()))
}

fn bundle_hash() -> String {
    let mut digest = Sha256::new();
    for file in BUNDLE_FILES {
        digest.update(file.path.as_bytes());
        digest.update([0]);
        digest.update(file.contents.as_bytes());
        digest.update([0]);
    }
    hex::encode(digest.finalize())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temporary_skills_root() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        env::temp_dir().join(format!("tapd-cli-skill-{}-{suffix}", std::process::id()))
    }

    #[test]
    fn custom_install_status_update_and_uninstall_are_managed() {
        let root = temporary_skills_root();
        let args = SkillTargetArgs {
            agent: SkillAgent::Auto,
            scope: SkillScope::User,
            dir: Some(root.clone()),
        };

        let _ = install(&args, false).expect("install bundle");
        let skill_dir = root.join(SKILL_NAME);
        assert!(skill_dir.join("SKILL.md").is_file());
        assert!(skill_dir.join(MANIFEST_NAME).is_file());

        let SkillOutput::Json(current_status) = status(&args).expect("read status") else {
            panic!("status must be JSON");
        };
        assert_eq!(current_status["targets"][0]["state"], "current");
        fs::write(skill_dir.join("SKILL.md"), "locally modified").expect("modify fixture");
        let SkillOutput::Json(modified_status) = status(&args).expect("read modified status")
        else {
            panic!("status must be JSON");
        };
        assert_eq!(modified_status["targets"][0]["state"], "modified");
        assert!(update(&args, false).is_err());
        let _ = update(&args, true).expect("force update bundle");
        let _ = uninstall(&args).expect("uninstall bundle");
        assert!(!skill_dir.exists());
        fs::remove_dir(&root).expect("remove empty fixture root");
    }

    #[test]
    fn embedded_resources_are_readable_and_path_traversal_is_rejected() {
        let SkillOutput::Plain(contents) = read("SKILL.md").expect("read entrypoint") else {
            panic!("read must return plain text");
        };
        let mut lines = contents.lines();
        assert_eq!(lines.next(), Some("---"));
        assert_eq!(lines.next(), Some("name: tapd-cli"));
        assert!(read("../SKILL.md").is_err());
    }
}
