use std::{
    collections::BTreeSet,
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

use agent_core::{
    resolve_operator_path, resolve_path_within_root, resolve_relative_path_within_root,
    validate_single_path_component, InstalledPluginConfig, PluginDoctorReport,
    PluginInstallRequest, PluginManifest, PluginPermissions, PluginSourceKind, PluginUpdateRequest,
    PLUGIN_HOST_VERSION, PLUGIN_MANIFEST_FILE_NAME, PLUGIN_SCHEMA_VERSION,
};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::AppPaths;

const MARKETPLACE_ENV_VAR: &str = "AGENT_PLUGIN_MARKETPLACE_INDEX";

#[derive(Debug, Clone)]
pub struct ResolvedPluginSource {
    pub manifest: PluginManifest,
    pub source_kind: PluginSourceKind,
    pub source_reference: String,
    pub source_path: PathBuf,
    pub source_root: PathBuf,
}

#[derive(Debug, Clone)]
struct GitSourceReference {
    repository: String,
    reference: Option<String>,
    subdir: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
struct MarketplaceIndex {
    #[serde(default)]
    plugins: Vec<MarketplaceIndexEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct MarketplaceIndexEntry {
    id: String,
    #[serde(default)]
    version: Option<String>,
    source: String,
}

pub fn load_plugin_manifest_from_source(
    source_path: impl AsRef<Path>,
) -> Result<(PluginManifest, PathBuf, PathBuf)> {
    let source_path = source_path.as_ref();
    let source_path = fs::canonicalize(source_path).with_context(|| {
        format!(
            "failed to resolve plugin source path '{}'",
            source_path.display()
        )
    })?;
    let (source_root, manifest_path) = resolve_plugin_source(&source_path)?;
    let manifest = read_plugin_manifest(&manifest_path)?;
    validate_plugin_manifest(&manifest)?;
    Ok((manifest, source_path, source_root))
}

pub fn resolve_plugin_install_request(
    paths: &AppPaths,
    request: &PluginInstallRequest,
) -> Result<ResolvedPluginSource> {
    let source_reference = request
        .source_reference()
        .ok_or_else(|| anyhow!("plugin source is required"))?;
    resolve_plugin_source_reference(paths, &source_reference)
}

pub fn resolve_plugin_source_reference(
    paths: &AppPaths,
    source_reference: &str,
) -> Result<ResolvedPluginSource> {
    let source_reference = source_reference.trim();
    if source_reference.is_empty() {
        bail!("plugin source is required");
    }
    if source_reference.starts_with("git+") {
        return resolve_git_plugin_source(paths, source_reference);
    }
    if source_reference.starts_with("market:") || source_reference.starts_with("marketplace:") {
        return resolve_marketplace_plugin_source(paths, source_reference);
    }
    resolve_local_plugin_source(source_reference)
}

pub fn install_plugin_package(
    paths: &AppPaths,
    request: &PluginInstallRequest,
    existing: Option<&InstalledPluginConfig>,
) -> Result<InstalledPluginConfig> {
    paths.ensure()?;
    let resolved = resolve_plugin_install_request(paths, request)?;
    validate_single_path_component(&resolved.manifest.id, "plugin manifest id")?;
    let plugin_root = paths.validated_plugin_dir()?;
    let install_dir = resolve_relative_path_within_root(
        &plugin_root,
        Path::new(&resolved.manifest.id),
        "plugin install directory",
    )?;
    let temp_dir = resolve_relative_path_within_root(
        &plugin_root,
        Path::new(&format!(
            ".install-{}-{}",
            resolved.manifest.id,
            Uuid::new_v4()
        )),
        "plugin staging directory",
    )?;

    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).with_context(|| {
            format!(
                "failed to clear stale plugin staging directory '{}'",
                temp_dir.display()
            )
        })?;
    }

    copy_dir_recursive(&resolved.source_root, &temp_dir)?;

    let staged_manifest = temp_dir.join(PLUGIN_MANIFEST_FILE_NAME);
    if !staged_manifest.exists() {
        bail!(
            "plugin package for '{}' did not include {}",
            resolved.manifest.id,
            PLUGIN_MANIFEST_FILE_NAME
        );
    }

    if install_dir.exists() {
        fs::remove_dir_all(&install_dir).with_context(|| {
            format!(
                "failed to replace existing plugin install '{}'",
                install_dir.display()
            )
        })?;
    }
    fs::rename(&temp_dir, &install_dir).with_context(|| {
        format!(
            "failed to finalize plugin install into '{}'",
            install_dir.display()
        )
    })?;
    let install_dir = fs::canonicalize(&install_dir).unwrap_or(install_dir);
    let integrity_sha256 = package_integrity_sha256(&install_dir)?;

    let now = Utc::now();
    let installed_at = existing.map(|plugin| plugin.installed_at).unwrap_or(now);
    let enabled = request
        .enabled
        .unwrap_or_else(|| existing.is_some_and(|plugin| plugin.enabled));
    let declared_permissions = resolved.manifest.declared_permissions();
    let preserved_review = existing
        .filter(|plugin| plugin.review_current() && plugin.integrity_sha256 == integrity_sha256);
    let trusted = request
        .trusted
        .unwrap_or_else(|| preserved_review.is_some());
    let granted_permissions = if trusted {
        request
            .granted_permissions
            .as_ref()
            .cloned()
            .or_else(|| {
                preserved_review.map(|plugin| {
                    plugin
                        .granted_permissions
                        .intersection(&declared_permissions)
                })
            })
            .unwrap_or_default()
            .intersection(&declared_permissions)
    } else {
        PluginPermissions::default()
    };
    let reviewed_integrity_sha256 = if trusted {
        integrity_sha256.clone()
    } else {
        String::new()
    };
    let reviewed_at = if trusted {
        if request.trusted.is_none() && request.granted_permissions.is_none() {
            preserved_review.and_then(|plugin| plugin.reviewed_at)
        } else {
            Some(now)
        }
    } else {
        None
    };
    let pinned = request.pinned || existing.is_some_and(|plugin| plugin.pinned);

    Ok(InstalledPluginConfig {
        id: resolved.manifest.id.clone(),
        manifest: resolved.manifest,
        source_kind: resolved.source_kind,
        install_dir,
        source_reference: resolved.source_reference,
        source_path: resolved.source_path,
        integrity_sha256,
        enabled,
        trusted,
        granted_permissions,
        reviewed_integrity_sha256,
        reviewed_at,
        pinned,
        installed_at,
        updated_at: now,
    })
}

pub fn update_plugin_package(
    paths: &AppPaths,
    plugin: &InstalledPluginConfig,
    request: &PluginUpdateRequest,
) -> Result<InstalledPluginConfig> {
    let source_reference = request.source_reference().unwrap_or_else(|| {
        if plugin.source_reference.trim().is_empty() {
            plugin.source_path.display().to_string()
        } else {
            plugin.source_reference.clone()
        }
    });
    install_plugin_package(
        paths,
        &PluginInstallRequest {
            source: Some(source_reference),
            source_path: None,
            enabled: Some(plugin.enabled),
            trusted: None,
            granted_permissions: None,
            pinned: plugin.pinned,
        },
        Some(plugin),
    )
}

pub fn uninstall_plugin_package(paths: &AppPaths, plugin: &InstalledPluginConfig) -> Result<()> {
    let plugin_root = paths.validated_plugin_dir()?;
    let install_dir = resolve_path_within_root(
        &plugin_root,
        &plugin.install_dir,
        "plugin install directory",
    )?;
    if install_dir.exists() {
        fs::remove_dir_all(&install_dir).with_context(|| {
            format!(
                "failed to remove plugin install directory '{}'",
                install_dir.display()
            )
        })?;
    }
    Ok(())
}

pub fn doctor_plugin(plugin: &InstalledPluginConfig) -> PluginDoctorReport {
    let mut issues = Vec::new();
    let mut notes = Vec::new();
    let declared_permissions = plugin.declared_permissions();
    let granted_permissions = plugin
        .granted_permissions
        .intersection(&declared_permissions);

    if plugin.manifest.schema_version != PLUGIN_SCHEMA_VERSION {
        issues.push(format!(
            "unsupported schema version {}",
            plugin.manifest.schema_version
        ));
    }
    if let Some(min_host_version) = plugin.manifest.compatibility.min_host_version {
        if min_host_version > PLUGIN_HOST_VERSION {
            issues.push(format!(
                "plugin requires host version >= {}, current host version is {}",
                min_host_version, PLUGIN_HOST_VERSION
            ));
        }
    }
    if let Some(max_host_version) = plugin.manifest.compatibility.max_host_version {
        if max_host_version < PLUGIN_HOST_VERSION {
            issues.push(format!(
                "plugin supports host version <= {}, current host version is {}",
                max_host_version, PLUGIN_HOST_VERSION
            ));
        }
    }
    if plugin.manifest.id != plugin.id {
        issues.push(format!(
            "config id '{}' does not match manifest id '{}'",
            plugin.id, plugin.manifest.id
        ));
    }
    if plugin.manifest.capability_count() == 0 {
        issues.push("manifest does not declare any capabilities".to_string());
    }
    if plugin.integrity_sha256.trim().is_empty() {
        issues.push("plugin integrity hash is missing".to_string());
    }
    if plugin.source_reference.trim().is_empty() {
        issues.push("plugin source reference is missing".to_string());
    }
    if !plugin.install_dir.exists() {
        issues.push(format!(
            "install dir '{}' is missing",
            plugin.install_dir.display()
        ));
    } else {
        let manifest_path = plugin.install_dir.join(PLUGIN_MANIFEST_FILE_NAME);
        if !manifest_path.exists() {
            issues.push(format!(
                "install dir is missing {}",
                PLUGIN_MANIFEST_FILE_NAME
            ));
        } else if let Ok(current_integrity) = package_integrity_sha256(&plugin.install_dir) {
            if !plugin.integrity_sha256.trim().is_empty()
                && current_integrity != plugin.integrity_sha256
            {
                issues.push("plugin contents no longer match recorded integrity hash".to_string());
            }
        } else {
            issues.push("failed to compute plugin integrity hash".to_string());
        }
    }

    if plugin.trusted && plugin.reviewed_integrity_sha256.trim().is_empty() {
        issues.push("plugin is trusted but has no recorded integrity review".to_string());
    } else if !plugin.reviewed_integrity_sha256.trim().is_empty()
        && plugin.reviewed_integrity_sha256 != plugin.integrity_sha256
    {
        issues.push("plugin contents changed since the last trust review".to_string());
    }

    if let Err(error) = validate_plugin_manifest(&plugin.manifest) {
        issues.push(error.to_string());
    }

    for tool in &plugin.manifest.tools {
        if let Some(cwd) = &tool.cwd {
            let resolved = resolve_plugin_path(&plugin.install_dir, cwd);
            if !resolved.exists() {
                issues.push(format!(
                    "tool '{}' cwd '{}' does not exist",
                    tool.name,
                    resolved.display()
                ));
            }
        }

        let command_path = resolve_plugin_command_path(&plugin.install_dir, &tool.command);
        if let Some(command_path) = command_path {
            if !command_path.exists() {
                issues.push(format!(
                    "tool '{}' command '{}' does not exist",
                    tool.name,
                    command_path.display()
                ));
            }
        }
    }

    for connector in &plugin.manifest.connectors {
        if let Some(cwd) = &connector.cwd {
            let resolved = resolve_plugin_path(&plugin.install_dir, cwd);
            if !resolved.exists() {
                issues.push(format!(
                    "connector '{}' cwd '{}' does not exist",
                    connector.id,
                    resolved.display()
                ));
            }
        }

        let command_path = resolve_plugin_command_path(&plugin.install_dir, &connector.command);
        if let Some(command_path) = command_path {
            if !command_path.exists() {
                issues.push(format!(
                    "connector '{}' command '{}' does not exist",
                    connector.id,
                    command_path.display()
                ));
            }
        }
    }

    for adapter in &plugin.manifest.provider_adapters {
        if let Some(cwd) = &adapter.cwd {
            let resolved = resolve_plugin_path(&plugin.install_dir, cwd);
            if !resolved.exists() {
                issues.push(format!(
                    "provider adapter '{}' cwd '{}' does not exist",
                    adapter.id,
                    resolved.display()
                ));
            }
        }

        let command_path = resolve_plugin_command_path(&plugin.install_dir, &adapter.command);
        if let Some(command_path) = command_path {
            if !command_path.exists() {
                issues.push(format!(
                    "provider adapter '{}' command '{}' does not exist",
                    adapter.id,
                    command_path.display()
                ));
            }
        }
    }

    if plugin.enabled && !plugin.review_current() {
        issues.push(
            "plugin is enabled but not currently reviewed for this integrity hash".to_string(),
        );
    }
    let missing_grants = declared_permissions.missing_from(&granted_permissions);
    if plugin.review_current() && !missing_grants.is_empty() {
        issues.push(format!(
            "permission grants missing for declared capabilities: {}",
            missing_grants.join(", ")
        ));
    }
    if plugin.runtime_projection_ready() && missing_grants.is_empty() {
        notes.push("plugin runtime projection is active".to_string());
    }

    let ok = issues.is_empty();
    let detail = if ok && notes.is_empty() {
        "ready".to_string()
    } else {
        issues
            .into_iter()
            .chain(notes)
            .collect::<Vec<_>>()
            .join("; ")
    };

    PluginDoctorReport {
        id: plugin.id.clone(),
        name: plugin.manifest.name.clone(),
        version: plugin.manifest.version.clone(),
        enabled: plugin.enabled,
        trusted: plugin.trusted,
        runtime_ready: plugin.runtime_projection_ready() && missing_grants.is_empty(),
        ok,
        detail,
        tools: plugin.manifest.tools.len(),
        connectors: plugin.manifest.connectors.len(),
        provider_adapters: plugin.manifest.provider_adapters.len(),
        integrity_sha256: plugin.integrity_sha256.clone(),
        source_kind: plugin.source_kind.clone(),
        declared_permissions,
        granted_permissions,
        reviewed_at: plugin.reviewed_at,
    }
}

pub fn resolve_plugin_path(base: &Path, declared: &Path) -> PathBuf {
    if declared.is_absolute() {
        declared.to_path_buf()
    } else {
        base.join(declared)
    }
}

pub fn resolve_plugin_command(base: &Path, command: &str) -> String {
    resolve_plugin_command_path(base, command)
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| command.to_string())
}

fn resolve_plugin_source(source_path: &Path) -> Result<(PathBuf, PathBuf)> {
    if source_path.is_dir() {
        let manifest_path = source_path.join(PLUGIN_MANIFEST_FILE_NAME);
        if !manifest_path.exists() {
            bail!(
                "plugin directory '{}' does not contain {}",
                source_path.display(),
                PLUGIN_MANIFEST_FILE_NAME
            );
        }
        return Ok((source_path.to_path_buf(), manifest_path));
    }

    if !source_path.is_file() {
        bail!(
            "plugin source '{}' is not a file or directory",
            source_path.display()
        );
    }

    let file_name = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("plugin manifest path contains invalid unicode"))?;
    if file_name != PLUGIN_MANIFEST_FILE_NAME {
        bail!(
            "plugin source file must be named {}",
            PLUGIN_MANIFEST_FILE_NAME
        );
    }

    let source_root = source_path
        .parent()
        .ok_or_else(|| anyhow!("plugin manifest path did not have a parent directory"))?;
    Ok((source_root.to_path_buf(), source_path.to_path_buf()))
}

fn resolve_local_plugin_source(source_reference: &str) -> Result<ResolvedPluginSource> {
    let source_path = PathBuf::from(source_reference);
    let (manifest, source_path, source_root) = load_plugin_manifest_from_source(&source_path)?;
    Ok(ResolvedPluginSource {
        manifest,
        source_kind: PluginSourceKind::LocalPath,
        source_reference: source_reference.to_string(),
        source_path,
        source_root,
    })
}

fn resolve_git_plugin_source(
    paths: &AppPaths,
    source_reference: &str,
) -> Result<ResolvedPluginSource> {
    let git = parse_git_source_reference(source_reference)?;
    let cache_root = plugin_source_cache_dir(paths)?;
    fs::create_dir_all(&cache_root).with_context(|| {
        format!(
            "failed to create plugin source cache directory '{}'",
            cache_root.display()
        )
    })?;
    let checkout_dir = resolve_relative_path_within_root(
        &cache_root,
        Path::new(&format!("git-{}", stable_hash(source_reference))),
        "plugin git checkout directory",
    )?;
    if checkout_dir.exists() {
        fs::remove_dir_all(&checkout_dir).with_context(|| {
            format!(
                "failed to clear existing plugin source checkout '{}'",
                checkout_dir.display()
            )
        })?;
    }

    let checkout_dir_text = checkout_dir.display().to_string();
    if local_git_repository(&git.repository) {
        run_git_command(
            None,
            &["clone", "--no-local", &git.repository, &checkout_dir_text],
        )?;
    } else {
        run_git_command(None, &["clone", &git.repository, &checkout_dir_text])?;
    }
    if let Some(reference) = git.reference.as_deref() {
        run_git_command(Some(&checkout_dir), &["checkout", reference])?;
    }

    let source_probe = git
        .subdir
        .as_ref()
        .map(|subdir| {
            resolve_relative_path_within_root(
                &checkout_dir,
                subdir,
                "plugin git source subdirectory",
            )
        })
        .transpose()?
        .unwrap_or_else(|| checkout_dir.clone());
    let source_probe = fs::canonicalize(&source_probe).with_context(|| {
        format!(
            "failed to resolve plugin source inside git checkout '{}'",
            source_probe.display()
        )
    })?;
    let (source_root, manifest_path) = resolve_plugin_source(&source_probe)?;
    let manifest = read_plugin_manifest(&manifest_path)?;
    validate_plugin_manifest(&manifest)?;
    Ok(ResolvedPluginSource {
        manifest,
        source_kind: PluginSourceKind::GitRepo,
        source_reference: source_reference.to_string(),
        source_path: checkout_dir,
        source_root,
    })
}

fn resolve_marketplace_plugin_source(
    paths: &AppPaths,
    source_reference: &str,
) -> Result<ResolvedPluginSource> {
    let (plugin_id, requested_version) = parse_marketplace_reference(source_reference)?;
    let (index, index_path) = load_marketplace_index(paths)?;
    let entry = index
        .plugins
        .iter()
        .find(|entry| {
            entry.id == plugin_id
                && match requested_version.as_deref() {
                    Some(version) => entry.version.as_deref() == Some(version),
                    None => true,
                }
        })
        .ok_or_else(|| {
            anyhow!(
                "plugin '{}' was not found in marketplace index '{}'",
                plugin_id,
                index_path.display()
            )
        })?;
    let entry_source = normalize_marketplace_source(
        &entry.source,
        index_path.parent().unwrap_or_else(|| Path::new(".")),
    )?;
    let mut resolved = resolve_plugin_source_reference(paths, &entry_source)?;
    resolved.source_kind = PluginSourceKind::Marketplace;
    resolved.source_reference = source_reference.to_string();
    Ok(resolved)
}

fn parse_git_source_reference(source_reference: &str) -> Result<GitSourceReference> {
    let remainder = source_reference
        .strip_prefix("git+")
        .ok_or_else(|| anyhow!("git plugin sources must start with 'git+'"))?;
    let (repo_and_ref, subdir) = remainder
        .split_once("::")
        .map(|(repo_and_ref, subdir)| (repo_and_ref, Some(PathBuf::from(subdir))))
        .unwrap_or((remainder, None));
    let (repository, reference) = repo_and_ref
        .rsplit_once('#')
        .map(|(repository, reference)| (repository, Some(reference.to_string())))
        .unwrap_or((repo_and_ref, None));
    if repository.trim().is_empty() {
        bail!("git plugin source is missing a repository reference");
    }
    Ok(GitSourceReference {
        repository: repository.to_string(),
        reference,
        subdir,
    })
}

fn parse_marketplace_reference(source_reference: &str) -> Result<(String, Option<String>)> {
    let remainder = source_reference
        .strip_prefix("market:")
        .or_else(|| source_reference.strip_prefix("marketplace:"))
        .ok_or_else(|| anyhow!("marketplace sources must start with 'market:'"))?;
    let (plugin_id, version) = remainder
        .split_once('@')
        .map(|(plugin_id, version)| (plugin_id, Some(version.to_string())))
        .unwrap_or((remainder, None));
    if plugin_id.trim().is_empty() {
        bail!("marketplace source is missing a plugin id");
    }
    validate_single_path_component(plugin_id, "marketplace plugin id")?;
    Ok((plugin_id.to_string(), version))
}

fn load_marketplace_index(paths: &AppPaths) -> Result<(MarketplaceIndex, PathBuf)> {
    let index_path = env::var_os(MARKETPLACE_ENV_VAR)
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.config_dir.join("plugin-marketplace.json"));
    let index_path = fs::canonicalize(&index_path).with_context(|| {
        format!(
            "failed to resolve plugin marketplace index '{}'",
            index_path.display()
        )
    })?;
    let content = fs::read_to_string(&index_path).with_context(|| {
        format!(
            "failed to read plugin marketplace index '{}'",
            index_path.display()
        )
    })?;
    let index = serde_json::from_str::<MarketplaceIndex>(&content).with_context(|| {
        format!(
            "failed to parse plugin marketplace index '{}'",
            index_path.display()
        )
    })?;
    Ok((index, index_path))
}

fn normalize_marketplace_source(source: &str, base_dir: &Path) -> Result<String> {
    if source.starts_with("market:") || source.starts_with("marketplace:") {
        bail!("marketplace entries may not reference other marketplace entries");
    }
    if source.starts_with("git+") {
        return Ok(source.to_string());
    }
    let source_path = PathBuf::from(source);
    if source_path.is_absolute() {
        Ok(source.to_string())
    } else {
        Ok(base_dir.join(source_path).display().to_string())
    }
}

fn plugin_source_cache_dir(paths: &AppPaths) -> Result<PathBuf> {
    let data_dir = paths.validated_data_dir()?;
    resolve_relative_path_within_root(
        &data_dir,
        Path::new("plugin-sources"),
        "plugin source cache directory",
    )
}

fn stable_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn local_git_repository(repository: &str) -> bool {
    Path::new(repository).exists()
}

fn run_git_command(current_dir: Option<&Path>, args: &[&str]) -> Result<()> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(current_dir) = current_dir {
        command.current_dir(current_dir);
    }
    let output = command.output().context("failed to execute git")?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if stderr.is_empty() { stdout } else { stderr };
    bail!(
        "git {} failed: {}",
        args.join(" "),
        if detail.is_empty() {
            output.status.to_string()
        } else {
            detail
        }
    )
}

fn read_plugin_manifest(manifest_path: &Path) -> Result<PluginManifest> {
    let content = fs::read_to_string(manifest_path).with_context(|| {
        format!(
            "failed to read plugin manifest '{}'",
            manifest_path.display()
        )
    })?;
    serde_json::from_str(&content).with_context(|| {
        format!(
            "failed to parse plugin manifest '{}'",
            manifest_path.display()
        )
    })
}

fn validate_plugin_manifest(manifest: &PluginManifest) -> Result<()> {
    if manifest.schema_version != PLUGIN_SCHEMA_VERSION {
        bail!(
            "plugin '{}' uses unsupported schema version {}",
            manifest.id,
            manifest.schema_version
        );
    }
    if manifest.id.trim().is_empty() {
        bail!("plugin manifest id must not be empty");
    }
    validate_single_path_component(&manifest.id, "plugin manifest id")?;
    if !manifest
        .id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        bail!(
            "plugin '{}' contains unsupported characters, only letters, numbers, '.', '_' and '-' are allowed",
            manifest.id
        );
    }
    if manifest.name.trim().is_empty() {
        bail!("plugin '{}' name must not be empty", manifest.id);
    }
    if manifest.version.trim().is_empty() {
        bail!("plugin '{}' version must not be empty", manifest.id);
    }
    if manifest.description.trim().is_empty() {
        bail!("plugin '{}' description must not be empty", manifest.id);
    }
    if let (Some(min), Some(max)) = (
        manifest.compatibility.min_host_version,
        manifest.compatibility.max_host_version,
    ) {
        if min > max {
            bail!(
                "plugin '{}' declares min_host_version {} greater than max_host_version {}",
                manifest.id,
                min,
                max
            );
        }
    }
    if manifest.capability_count() == 0 {
        bail!(
            "plugin '{}' must declare at least one capability",
            manifest.id
        );
    }

    let mut tool_names = BTreeSet::new();
    for tool in &manifest.tools {
        if tool.name.trim().is_empty() {
            bail!(
                "plugin '{}' contains a tool with an empty name",
                manifest.id
            );
        }
        if !tool_names.insert(tool.name.clone()) {
            bail!(
                "plugin '{}' declares duplicate tool '{}'",
                manifest.id,
                tool.name
            );
        }
        if tool.description.trim().is_empty() {
            bail!(
                "plugin '{}' tool '{}' description must not be empty",
                manifest.id,
                tool.name
            );
        }
        if tool.command.trim().is_empty() {
            bail!(
                "plugin '{}' tool '{}' command must not be empty",
                manifest.id,
                tool.name
            );
        }
        let _ = serde_json::from_str::<serde_json::Value>(&tool.input_schema_json).with_context(
            || {
                format!(
                    "plugin '{}' tool '{}' input schema is not valid JSON",
                    manifest.id, tool.name
                )
            },
        )?;
        if let Some(timeout_seconds) = tool.timeout_seconds {
            if timeout_seconds == 0 || timeout_seconds > 600 {
                bail!(
                    "plugin '{}' tool '{}' timeout_seconds must be between 1 and 600 when set",
                    manifest.id,
                    tool.name
                );
            }
        }
    }

    let mut connector_ids = BTreeSet::new();
    for connector in &manifest.connectors {
        if connector.id.trim().is_empty() {
            bail!(
                "plugin '{}' contains a connector with an empty id",
                manifest.id
            );
        }
        if !connector_ids.insert(connector.id.clone()) {
            bail!(
                "plugin '{}' declares duplicate connector '{}'",
                manifest.id,
                connector.id
            );
        }
        if connector.description.trim().is_empty() {
            bail!(
                "plugin '{}' connector '{}' description must not be empty",
                manifest.id,
                connector.id
            );
        }
        if connector.command.trim().is_empty() {
            bail!(
                "plugin '{}' connector '{}' command must not be empty",
                manifest.id,
                connector.id
            );
        }
        if let Some(timeout_seconds) = connector.timeout_seconds {
            if timeout_seconds == 0 || timeout_seconds > 600 {
                bail!(
                    "plugin '{}' connector '{}' timeout_seconds must be between 1 and 600 when set",
                    manifest.id,
                    connector.id
                );
            }
        }
    }

    let mut provider_ids = BTreeSet::new();
    for adapter in &manifest.provider_adapters {
        if adapter.id.trim().is_empty() {
            bail!(
                "plugin '{}' contains a provider adapter with an empty id",
                manifest.id
            );
        }
        if !provider_ids.insert(adapter.id.clone()) {
            bail!(
                "plugin '{}' declares duplicate provider adapter '{}'",
                manifest.id,
                adapter.id
            );
        }
        if adapter.description.trim().is_empty() {
            bail!(
                "plugin '{}' provider adapter '{}' description must not be empty",
                manifest.id,
                adapter.id
            );
        }
        if adapter.command.trim().is_empty() {
            bail!(
                "plugin '{}' provider adapter '{}' command must not be empty",
                manifest.id,
                adapter.id
            );
        }
        if adapter
            .default_model
            .as_deref()
            .is_some_and(|model| model.trim().is_empty())
        {
            bail!(
                "plugin '{}' provider adapter '{}' default_model must not be empty when set",
                manifest.id,
                adapter.id
            );
        }
        if let Some(timeout_seconds) = adapter.timeout_seconds {
            if timeout_seconds == 0 || timeout_seconds > 600 {
                bail!(
                    "plugin '{}' provider adapter '{}' timeout_seconds must be between 1 and 600 when set",
                    manifest.id,
                    adapter.id
                );
            }
        }
    }

    Ok(())
}

fn resolve_plugin_command_path(base: &Path, command: &str) -> Option<PathBuf> {
    let declared = Path::new(command);
    if declared.is_absolute() {
        return Some(declared.to_path_buf());
    }

    let nested = base.join(declared);
    if nested.exists() {
        return Some(nested);
    }

    let looks_like_relative = command.starts_with('.')
        || command.contains('/')
        || command.contains('\\')
        || declared.components().count() > 1;

    looks_like_relative.then_some(nested)
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<()> {
    let source_root = resolve_operator_path(source, "plugin source directory")?;
    let target_root = resolve_operator_path(target, "plugin target directory")?;
    copy_dir_recursive_inner(&source_root, &source_root, &target_root, &target_root)
}

fn copy_dir_recursive_inner(
    source_root: &Path,
    source_dir: &Path,
    target_root: &Path,
    target_dir: &Path,
) -> Result<()> {
    let source_dir = resolve_path_within_root(source_root, source_dir, "plugin source directory")?;
    let target_dir = resolve_path_within_root(target_root, target_dir, "plugin target directory")?;

    fs::create_dir_all(&target_dir).with_context(|| {
        format!(
            "failed to create plugin target directory '{}'",
            target_dir.display()
        )
    })?;

    for entry in fs::read_dir(&source_dir).with_context(|| {
        format!(
            "failed to read plugin source directory '{}'",
            source_dir.display()
        )
    })? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let source_path =
            resolve_path_within_root(source_root, &entry.path(), "plugin source entry")?;
        let target_path = resolve_relative_path_within_root(
            &target_dir,
            Path::new(&entry.file_name()),
            "plugin target entry",
        )?;

        if file_type.is_dir() {
            copy_dir_recursive_inner(source_root, &source_path, target_root, &target_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &target_path).with_context(|| {
                format!(
                    "failed to copy plugin file '{}' to '{}'",
                    source_path.display(),
                    target_path.display()
                )
            })?;
        } else {
            bail!(
                "plugin package contains unsupported non-file entry '{}'",
                source_path.display()
            );
        }
    }

    Ok(())
}

fn package_integrity_sha256(root: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_plugin_files(root, root, &mut files)?;
    files.sort();

    let mut hasher = Sha256::new();
    for relative_path in files {
        hasher.update(relative_path.to_string_lossy().as_bytes());
        let mut file = fs::File::open(root.join(&relative_path)).with_context(|| {
            format!(
                "failed to open plugin file '{}' while hashing package",
                root.join(&relative_path).display()
            )
        })?;
        let mut buffer = [0_u8; 8192];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_plugin_files(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(current).with_context(|| {
        format!(
            "failed to read plugin directory '{}' while hashing package",
            current.display()
        )
    })? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_plugin_files(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .map(Path::to_path_buf)
                .unwrap_or(path.clone());
            files.push(relative);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use agent_core::{
        ConnectorKind, PluginCompatibility, PluginConnectorManifest, PluginPermissions,
        PluginProviderAdapterManifest, PluginToolManifest, ProviderKind,
    };

    fn temp_paths() -> AppPaths {
        let root = std::env::temp_dir().join(format!("agent-plugin-test-{}", Uuid::new_v4()));
        let paths = AppPaths::under_root(root);
        paths.ensure().unwrap();
        paths
    }

    fn write_manifest(dir: &Path, manifest: &PluginManifest) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join(PLUGIN_MANIFEST_FILE_NAME),
            serde_json::to_vec_pretty(manifest).unwrap(),
        )
        .unwrap();
    }

    fn git_available() -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn sample_manifest() -> PluginManifest {
        PluginManifest {
            schema_version: PLUGIN_SCHEMA_VERSION,
            id: "echo-toolkit".to_string(),
            name: "Echo Toolkit".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: "Sample plugin".to_string(),
            homepage: None,
            compatibility: PluginCompatibility::default(),
            permissions: PluginPermissions::default(),
            tools: vec![PluginToolManifest {
                name: "echo_tool".to_string(),
                description: "Echo".to_string(),
                command: "python".to_string(),
                args: vec!["tool.py".to_string()],
                input_schema_json: "{\"type\":\"object\"}".to_string(),
                cwd: Some(PathBuf::from("bin")),
                permissions: PluginPermissions::default(),
                timeout_seconds: Some(30),
            }],
            connectors: vec![PluginConnectorManifest {
                id: "future-connector".to_string(),
                kind: ConnectorKind::Webhook,
                description: "Future".to_string(),
                command: "plugin-host".to_string(),
                args: Vec::new(),
                cwd: None,
                permissions: PluginPermissions::default(),
                timeout_seconds: None,
            }],
            provider_adapters: vec![PluginProviderAdapterManifest {
                id: "future-provider".to_string(),
                provider_kind: ProviderKind::OpenAiCompatible,
                description: "Future".to_string(),
                command: "plugin-host".to_string(),
                args: Vec::new(),
                cwd: None,
                permissions: PluginPermissions::default(),
                default_model: None,
                timeout_seconds: None,
            }],
        }
    }

    #[test]
    fn install_plugin_package_copies_source_tree() {
        let paths = temp_paths();
        let source_root = paths.root_dir.join("plugin-source");
        let manifest = sample_manifest();
        write_manifest(&source_root, &manifest);
        fs::create_dir_all(source_root.join("bin")).unwrap();
        fs::write(source_root.join("bin").join("tool.py"), "print('ok')").unwrap();

        let installed = install_plugin_package(
            &paths,
            &PluginInstallRequest {
                source: None,
                source_path: Some(source_root.clone()),
                enabled: Some(true),
                trusted: Some(true),
                granted_permissions: None,
                pinned: true,
            },
            None,
        )
        .unwrap();

        assert_eq!(installed.id, manifest.id);
        assert!(installed.install_dir.exists());
        assert!(installed
            .install_dir
            .join(PLUGIN_MANIFEST_FILE_NAME)
            .exists());
        assert!(installed.install_dir.join("bin").join("tool.py").exists());
        assert_eq!(installed.source_kind, PluginSourceKind::LocalPath);
        assert!(!installed.integrity_sha256.is_empty());
        assert!(installed.enabled);
        assert!(installed.trusted);
        assert_eq!(
            installed.reviewed_integrity_sha256,
            installed.integrity_sha256
        );
        assert!(installed.pinned);
    }

    #[test]
    fn doctor_plugin_flags_untrusted_enabled_runtime_block() {
        let paths = temp_paths();
        let source_root = paths.root_dir.join("plugin-source");
        let manifest = sample_manifest();
        write_manifest(&source_root, &manifest);
        fs::create_dir_all(source_root.join("bin")).unwrap();
        fs::write(source_root.join("bin").join("tool.py"), "print('ok')").unwrap();

        let installed = install_plugin_package(
            &paths,
            &PluginInstallRequest {
                source: None,
                source_path: Some(source_root),
                enabled: Some(true),
                trusted: Some(false),
                granted_permissions: None,
                pinned: false,
            },
            None,
        )
        .unwrap();

        let report = doctor_plugin(&installed);
        assert!(!report.ok);
        assert!(report.detail.contains("enabled but not currently reviewed"));
    }

    #[test]
    fn update_plugin_package_uses_existing_source_by_default() {
        let paths = temp_paths();
        let source_root = paths.root_dir.join("plugin-source");
        let manifest = sample_manifest();
        write_manifest(&source_root, &manifest);
        fs::create_dir_all(source_root.join("bin")).unwrap();
        fs::write(source_root.join("bin").join("tool.py"), "print('v1')").unwrap();

        let installed = install_plugin_package(
            &paths,
            &PluginInstallRequest {
                source: None,
                source_path: Some(source_root.clone()),
                enabled: Some(true),
                trusted: Some(true),
                granted_permissions: None,
                pinned: false,
            },
            None,
        )
        .unwrap();

        fs::write(source_root.join("bin").join("tool.py"), "print('v2')").unwrap();
        let updated =
            update_plugin_package(&paths, &installed, &PluginUpdateRequest::default()).unwrap();

        assert_eq!(updated.source_path, installed.source_path);
        assert_ne!(updated.integrity_sha256, installed.integrity_sha256);
    }

    #[test]
    fn update_plugin_package_invalidates_review_when_integrity_changes() {
        let paths = temp_paths();
        let source_root = paths.root_dir.join("plugin-source");
        let manifest = sample_manifest();
        write_manifest(&source_root, &manifest);
        fs::create_dir_all(source_root.join("bin")).unwrap();
        fs::write(source_root.join("bin").join("tool.py"), "print('v1')").unwrap();

        let installed = install_plugin_package(
            &paths,
            &PluginInstallRequest {
                source: None,
                source_path: Some(source_root.clone()),
                enabled: Some(true),
                trusted: Some(true),
                granted_permissions: None,
                pinned: false,
            },
            None,
        )
        .unwrap();

        fs::write(source_root.join("bin").join("tool.py"), "print('v2')").unwrap();
        let updated =
            update_plugin_package(&paths, &installed, &PluginUpdateRequest::default()).unwrap();

        assert!(!updated.trusted);
        assert!(updated.reviewed_integrity_sha256.is_empty());
        assert!(updated.reviewed_at.is_none());
    }

    #[test]
    fn doctor_plugin_detects_integrity_mismatch_after_tamper() {
        let paths = temp_paths();
        let source_root = paths.root_dir.join("plugin-source");
        let manifest = sample_manifest();
        write_manifest(&source_root, &manifest);
        fs::create_dir_all(source_root.join("bin")).unwrap();
        fs::write(source_root.join("bin").join("tool.py"), "print('ok')").unwrap();

        let installed = install_plugin_package(
            &paths,
            &PluginInstallRequest {
                source: None,
                source_path: Some(source_root),
                enabled: Some(true),
                trusted: Some(true),
                granted_permissions: None,
                pinned: false,
            },
            None,
        )
        .unwrap();

        fs::write(
            installed.install_dir.join("bin").join("tool.py"),
            "print('tampered')",
        )
        .unwrap();
        let report = doctor_plugin(&installed);

        assert!(!report.ok);
        assert!(report.detail.contains("integrity"));
    }

    #[test]
    fn resolve_plugin_source_reference_supports_marketplace_entries() {
        let paths = temp_paths();
        let source_root = paths.root_dir.join("plugin-source");
        let manifest = sample_manifest();
        write_manifest(&source_root, &manifest);
        fs::create_dir_all(source_root.join("bin")).unwrap();
        fs::write(source_root.join("bin").join("tool.py"), "print('ok')").unwrap();
        fs::write(
            paths.config_dir.join("plugin-marketplace.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "plugins": [
                    {
                        "id": manifest.id,
                        "source": PathBuf::from("..").join("plugin-source"),
                    }
                ]
            }))
            .unwrap(),
        )
        .unwrap();

        let resolved = resolve_plugin_source_reference(&paths, "market:echo-toolkit").unwrap();

        assert_eq!(resolved.source_kind, PluginSourceKind::Marketplace);
        assert_eq!(resolved.source_reference, "market:echo-toolkit");
        assert_eq!(resolved.manifest.id, "echo-toolkit");
    }

    #[test]
    fn validate_plugin_manifest_rejects_path_like_id() {
        let mut manifest = sample_manifest();
        manifest.id = "../escape".to_string();

        let error = validate_plugin_manifest(&manifest).unwrap_err();

        assert!(error.to_string().contains("plugin manifest id"));
    }

    #[test]
    fn parse_marketplace_reference_rejects_path_like_id() {
        let error = parse_marketplace_reference("market:../../escape").unwrap_err();

        assert!(error.to_string().contains("marketplace plugin id"));
    }

    #[test]
    fn parse_git_source_reference_rejects_escape_subdir_during_resolution() {
        let paths = temp_paths();
        let checkout_dir = paths.data_dir.join("plugin-sources").join("git-checkout");
        fs::create_dir_all(&checkout_dir).unwrap();

        let git =
            parse_git_source_reference("git+https://example.invalid/repo.git::../escape").unwrap();
        let error = resolve_relative_path_within_root(
            &checkout_dir,
            git.subdir.as_deref().unwrap(),
            "plugin git source subdirectory",
        )
        .unwrap_err();

        assert!(error.to_string().contains("traversal"));
    }

    #[test]
    fn uninstall_plugin_package_rejects_escape_install_dir() {
        let paths = temp_paths();
        let manifest = sample_manifest();
        let plugin = InstalledPluginConfig {
            id: manifest.id.clone(),
            manifest,
            source_kind: PluginSourceKind::LocalPath,
            install_dir: paths.root_dir.join("..").join("escape"),
            source_reference: String::new(),
            source_path: paths.root_dir.join("plugin-source"),
            integrity_sha256: String::new(),
            enabled: false,
            trusted: false,
            granted_permissions: PluginPermissions::default(),
            reviewed_integrity_sha256: String::new(),
            reviewed_at: None,
            pinned: false,
            installed_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let error = uninstall_plugin_package(&paths, &plugin).unwrap_err();

        assert!(error.to_string().contains("escapes managed root"));
    }

    #[test]
    fn resolve_plugin_source_reference_supports_git_repositories() {
        if !git_available() {
            return;
        }

        let paths = temp_paths();
        let repo_root = paths.root_dir.join("plugin-repo");
        let manifest = sample_manifest();
        write_manifest(&repo_root, &manifest);
        fs::create_dir_all(repo_root.join("bin")).unwrap();
        fs::write(repo_root.join("bin").join("tool.py"), "print('ok')").unwrap();

        run_git_command(None, &["init", &repo_root.display().to_string()]).unwrap();
        run_git_command(
            Some(&repo_root),
            &["config", "user.email", "plugin-test@example.com"],
        )
        .unwrap();
        run_git_command(Some(&repo_root), &["config", "user.name", "Plugin Test"]).unwrap();
        run_git_command(Some(&repo_root), &["add", "."]).unwrap();
        run_git_command(Some(&repo_root), &["commit", "-m", "initial plugin"]).unwrap();

        let resolved =
            resolve_plugin_source_reference(&paths, &format!("git+{}", repo_root.display()))
                .unwrap();

        assert_eq!(resolved.source_kind, PluginSourceKind::GitRepo);
        assert_eq!(resolved.manifest.id, "echo-toolkit");
        assert!(resolved.source_path.exists());
        assert!(resolved.source_root.join("bin").join("tool.py").exists());
    }

    #[test]
    fn install_plugin_package_rejects_plugin_root_outside_data_dir() {
        let mut paths = temp_paths();
        let source_root = paths.root_dir.join("plugin-source");
        let manifest = sample_manifest();
        write_manifest(&source_root, &manifest);
        paths.plugin_dir = paths.root_dir.join("..").join("escape-plugins");

        let error = install_plugin_package(
            &paths,
            &PluginInstallRequest {
                source: None,
                source_path: Some(source_root),
                enabled: Some(true),
                trusted: Some(true),
                granted_permissions: None,
                pinned: false,
            },
            None,
        )
        .unwrap_err();

        assert!(error.to_string().contains("escapes managed root"));
    }
}
