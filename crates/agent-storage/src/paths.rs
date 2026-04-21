use super::*;
use agent_core::{
    resolve_operator_path, resolve_path_from_existing_parent, resolve_path_within_root,
    CONFIG_VERSION,
};
use serde::{Deserialize, Serialize};

const PATH_MIGRATION_SCHEMA_VERSION: u32 = 1;
const LEGACY_APP_NAME: &str = "Agent Builder";

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root_dir: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub plugin_dir: PathBuf,
    pub config_path: PathBuf,
    pub db_path: PathBuf,
    pub migration_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathMigrationRecord {
    pub schema_version: u32,
    pub migrated_at: chrono::DateTime<chrono::Utc>,
    pub legacy_root_dir: Option<String>,
    pub legacy_config_dir: String,
    pub legacy_data_dir: String,
    pub legacy_log_dir: String,
    #[serde(default)]
    pub moved_paths: Vec<String>,
    #[serde(default)]
    pub copied_paths: Vec<String>,
    #[serde(default)]
    pub skipped_existing: Vec<String>,
}

impl PathMigrationRecord {
    fn matches_legacy_paths(&self, legacy: &AppPaths) -> bool {
        if self.schema_version != PATH_MIGRATION_SCHEMA_VERSION {
            return false;
        }

        let root_matches = self
            .legacy_root_dir
            .as_deref()
            .map(Path::new)
            .is_none_or(|root| root == legacy.root_dir.as_path());

        root_matches
            && Path::new(&self.legacy_config_dir) == legacy.config_dir.as_path()
            && Path::new(&self.legacy_data_dir) == legacy.data_dir.as_path()
            && Path::new(&self.legacy_log_dir) == legacy.log_dir.as_path()
    }
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        #[cfg(windows)]
        if let Some(paths) = Self::windows_layout(APP_NAME) {
            return Ok(paths);
        }

        if let Some(dirs) = ProjectDirs::from("com", "NuclearAI", APP_NAME) {
            return Ok(Self::from_standard_dirs(
                dirs.config_dir().to_path_buf(),
                dirs.data_dir().to_path_buf(),
                dirs.data_local_dir().to_path_buf(),
            ));
        }

        Self::fallback().ok_or_else(|| anyhow!("failed to resolve application directories"))
    }

    pub fn under_root(root_dir: impl AsRef<Path>) -> Self {
        let root_dir = root_dir.as_ref().to_path_buf();
        let config_dir = root_dir.join("config");
        let data_dir = root_dir.join("data");
        let log_dir = root_dir.join("logs");
        let plugin_dir = data_dir.join("plugins");
        Self {
            config_path: config_dir.join("config.json"),
            db_path: data_dir.join("agent.db"),
            migration_path: config_dir.join("migration-state.json"),
            root_dir,
            config_dir,
            data_dir,
            log_dir,
            plugin_dir,
        }
    }

    fn from_standard_dirs(config_dir: PathBuf, data_dir: PathBuf, root_dir: PathBuf) -> Self {
        let log_dir = root_dir.join("logs");
        let plugin_dir = data_dir.join("plugins");
        Self {
            config_path: config_dir.join("config.json"),
            db_path: data_dir.join("agent.db"),
            migration_path: config_dir.join("migration-state.json"),
            root_dir,
            config_dir,
            data_dir,
            log_dir,
            plugin_dir,
        }
    }

    #[cfg(windows)]
    fn fallback() -> Option<Self> {
        let user_profile = env_path("USERPROFILE");
        let app_data = env_path("APPDATA").or_else(|| {
            user_profile
                .as_ref()
                .map(|path| path.join("AppData").join("Roaming"))
        })?;
        let local_app_data = env_path("LOCALAPPDATA")
            .or_else(|| {
                user_profile
                    .as_ref()
                    .map(|path| path.join("AppData").join("Local"))
            })
            .unwrap_or_else(|| app_data.clone());

        Some(Self::from_windows_roots(app_data, local_app_data))
    }

    #[cfg(not(windows))]
    fn fallback() -> Option<Self> {
        None
    }

    #[cfg(windows)]
    fn from_windows_roots(app_data: PathBuf, local_app_data: PathBuf) -> Self {
        let roaming_root = app_data.join("NuclearAI").join(APP_NAME);
        let local_root = local_app_data.join("NuclearAI").join(APP_NAME);
        Self::from_standard_dirs(
            roaming_root.join("config"),
            roaming_root.join("data"),
            local_root,
        )
    }

    #[cfg(windows)]
    fn windows_layout(app_name: &str) -> Option<Self> {
        let user_profile = env_path("USERPROFILE");
        let app_data = env_path("APPDATA").or_else(|| {
            user_profile
                .as_ref()
                .map(|path| path.join("AppData").join("Roaming"))
        })?;
        let local_app_data = env_path("LOCALAPPDATA")
            .or_else(|| {
                user_profile
                    .as_ref()
                    .map(|path| path.join("AppData").join("Local"))
            })
            .unwrap_or_else(|| app_data.clone());

        let roaming_root = app_data.join("NuclearAI").join(app_name);
        let local_root = local_app_data.join("NuclearAI").join(app_name);
        Some(Self::from_standard_dirs(
            roaming_root.join("config"),
            roaming_root.join("data"),
            local_root,
        ))
    }

    pub fn ensure(&self) -> Result<()> {
        let root_dir = self.validated_root_dir()?;
        let config_dir = self.validated_config_dir()?;
        let data_dir = self.validated_data_dir()?;
        let log_dir = self.validated_log_dir()?;
        let plugin_dir = self.validated_plugin_dir()?;

        fs::create_dir_all(&root_dir).context("failed to create root dir")?;
        fs::create_dir_all(&config_dir).context("failed to create config dir")?;
        fs::create_dir_all(&data_dir).context("failed to create data dir")?;
        fs::create_dir_all(&log_dir).context("failed to create log dir")?;
        fs::create_dir_all(&plugin_dir).context("failed to create plugin dir")?;
        Ok(())
    }

    pub fn migrate_legacy_state(&self) -> Result<Option<PathMigrationRecord>> {
        let existing_record = load_migration_record(&self.migration_path)?;
        self.migrate_legacy_candidates(self.legacy_candidates(), existing_record.as_ref())
    }

    fn migrate_legacy_candidates<I>(
        &self,
        candidates: I,
        existing_record: Option<&PathMigrationRecord>,
    ) -> Result<Option<PathMigrationRecord>>
    where
        I: IntoIterator<Item = Self>,
    {
        for legacy in candidates {
            if legacy.config_dir == self.config_dir
                && legacy.data_dir == self.data_dir
                && legacy.log_dir == self.log_dir
            {
                continue;
            }
            if existing_record.is_some_and(|record| record.matches_legacy_paths(&legacy)) {
                continue;
            }
            if !legacy.config_dir.exists() && !legacy.data_dir.exists() && !legacy.log_dir.exists()
            {
                continue;
            }

            let mut record = PathMigrationRecord {
                schema_version: PATH_MIGRATION_SCHEMA_VERSION,
                migrated_at: chrono::Utc::now(),
                legacy_root_dir: Some(legacy.root_dir.display().to_string()),
                legacy_config_dir: legacy.config_dir.display().to_string(),
                legacy_data_dir: legacy.data_dir.display().to_string(),
                legacy_log_dir: legacy.log_dir.display().to_string(),
                moved_paths: Vec::new(),
                copied_paths: Vec::new(),
                skipped_existing: Vec::new(),
            };

            migrate_tree(
                &legacy.config_dir,
                &self.config_dir,
                &mut record.moved_paths,
                &mut record.copied_paths,
                &mut record.skipped_existing,
            )?;
            migrate_tree(
                &legacy.data_dir,
                &self.data_dir,
                &mut record.moved_paths,
                &mut record.copied_paths,
                &mut record.skipped_existing,
            )?;
            migrate_tree(
                &legacy.log_dir,
                &self.log_dir,
                &mut record.moved_paths,
                &mut record.copied_paths,
                &mut record.skipped_existing,
            )?;

            if record.moved_paths.is_empty()
                && record.copied_paths.is_empty()
                && record.skipped_existing.is_empty()
            {
                continue;
            }

            self.ensure()?;
            let content = serde_json::to_string_pretty(&record)
                .context("failed to encode path migration state")?;
            write_atomic(&self.migration_path, content.as_bytes())?;
            return Ok(Some(record));
        }

        Ok(None)
    }

    fn legacy_candidates(&self) -> Vec<Self> {
        let mut candidates: Vec<Self> = Vec::new();
        #[cfg(not(windows))]
        if let Some(dirs) = ProjectDirs::from("com", "NuclearAI", LEGACY_APP_NAME) {
            candidates.push(Self::from_standard_dirs(
                dirs.config_dir().to_path_buf(),
                dirs.data_dir().to_path_buf(),
                dirs.data_local_dir().to_path_buf(),
            ));
        }
        #[cfg(windows)]
        if let Some(paths) = Self::windows_layout(LEGACY_APP_NAME) {
            if !candidates.iter().any(|candidate| {
                candidate.config_dir == paths.config_dir
                    && candidate.data_dir == paths.data_dir
                    && candidate.root_dir == paths.root_dir
            }) {
                candidates.push(paths);
            }
        }
        candidates
    }

    pub fn validated_root_dir(&self) -> Result<PathBuf> {
        resolve_operator_path(&self.root_dir, "application root directory")
    }

    pub fn validated_config_dir(&self) -> Result<PathBuf> {
        let root_dir = self.validated_root_dir()?;
        resolve_path_within_root(&root_dir, &self.config_dir, "configuration directory")
    }

    pub fn validated_data_dir(&self) -> Result<PathBuf> {
        let root_dir = self.validated_root_dir()?;
        resolve_path_within_root(&root_dir, &self.data_dir, "data directory")
    }

    pub fn validated_log_dir(&self) -> Result<PathBuf> {
        let root_dir = self.validated_root_dir()?;
        resolve_path_within_root(&root_dir, &self.log_dir, "log directory")
    }

    pub fn validated_plugin_dir(&self) -> Result<PathBuf> {
        let data_dir = self.validated_data_dir()?;
        resolve_path_within_root(&data_dir, &self.plugin_dir, "plugin directory")
    }

    pub fn validated_config_path(&self) -> Result<PathBuf> {
        let config_dir = self.validated_config_dir()?;
        resolve_path_from_existing_parent(&self.config_path, "configuration file")
            .and_then(|path| resolve_path_within_root(&config_dir, &path, "configuration file"))
    }
}

fn load_migration_record(migration_path: &Path) -> Result<Option<PathMigrationRecord>> {
    let content = match fs::read_to_string(migration_path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to read path migration state {}",
                    migration_path.display()
                )
            });
        }
    };

    match serde_json::from_str::<PathMigrationRecord>(&content) {
        Ok(record) => Ok(Some(record)),
        Err(_) => Ok(None),
    }
}

fn migrate_tree(
    legacy: &Path,
    canonical: &Path,
    moved_paths: &mut Vec<String>,
    copied_paths: &mut Vec<String>,
    skipped_existing: &mut Vec<String>,
) -> Result<()> {
    if !legacy.exists() {
        return Ok(());
    }

    if !canonical.exists() {
        let parent = canonical.parent().ok_or_else(|| {
            anyhow!(
                "canonical path {} has no parent directory",
                canonical.display()
            )
        })?;
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to prepare canonical parent directory {}",
                parent.display()
            )
        })?;
        match fs::rename(legacy, canonical) {
            Ok(()) => {
                moved_paths.push(format!("{} -> {}", legacy.display(), canonical.display()));
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(());
            }
            Err(_) => {
                copy_tree_if_missing(legacy, canonical, copied_paths, skipped_existing)?;
                return Ok(());
            }
        }
    }

    copy_tree_if_missing(legacy, canonical, copied_paths, skipped_existing)?;
    Ok(())
}

fn copy_tree_if_missing(
    source: &Path,
    destination: &Path,
    copied_paths: &mut Vec<String>,
    skipped_existing: &mut Vec<String>,
) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }

    if source.is_file() {
        if destination.exists() {
            skipped_existing.push(destination.display().to_string());
            return Ok(());
        }
        let parent = destination.parent().ok_or_else(|| {
            anyhow!(
                "destination file {} has no parent directory",
                destination.display()
            )
        })?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create destination parent {}", parent.display()))?;
        fs::copy(source, destination).with_context(|| {
            format!(
                "failed to copy {} to {}",
                source.display(),
                destination.display()
            )
        })?;
        copied_paths.push(format!("{} -> {}", source.display(), destination.display()));
        return Ok(());
    }

    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create directory {}", destination.display()))?;
    let entries = match fs::read_dir(source) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read directory {}", source.display()));
        }
    };
    for entry in entries {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        copy_tree_if_missing(
            &source_path,
            &destination_path,
            copied_paths,
            skipped_existing,
        )?;
    }
    Ok(())
}

#[cfg(windows)]
fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).and_then(|value| {
        if value.is_empty() {
            None
        } else {
            Some(PathBuf::from(value))
        }
    })
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn windows_fallback_matches_existing_roaming_layout() {
        use std::path::PathBuf;

        use agent_core::APP_NAME;

        let app_data = PathBuf::from(r"C:\Users\Test\AppData\Roaming");
        let local_app_data = PathBuf::from(r"C:\Users\Test\AppData\Local");
        let paths = super::AppPaths::from_windows_roots(app_data.clone(), local_app_data.clone());

        let roaming_root = app_data.join("NuclearAI").join(APP_NAME);
        let local_root = local_app_data.join("NuclearAI").join(APP_NAME);

        assert_eq!(paths.config_dir, roaming_root.join("config"));
        assert_eq!(paths.data_dir, roaming_root.join("data"));
        assert_eq!(paths.root_dir, local_root);
        assert_eq!(paths.log_dir, local_root.join("logs"));
        assert_eq!(paths.plugin_dir, roaming_root.join("data").join("plugins"));
        assert_eq!(
            paths.config_path,
            roaming_root.join("config").join("config.json")
        );
        assert_eq!(paths.db_path, roaming_root.join("data").join("agent.db"));
    }

    #[test]
    fn copy_tree_if_missing_tolerates_missing_source_directory() {
        let temp =
            std::env::temp_dir().join(format!("agent-storage-paths-test-{}", uuid::Uuid::new_v4()));
        let source = temp.join("missing");
        let destination = temp.join("destination");
        let mut copied_paths = Vec::new();
        let mut skipped_existing = Vec::new();

        super::copy_tree_if_missing(
            &source,
            &destination,
            &mut copied_paths,
            &mut skipped_existing,
        )
        .unwrap();

        assert!(!destination.exists());
        assert!(copied_paths.is_empty());
        assert!(skipped_existing.is_empty());

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn migrate_legacy_candidates_is_idempotent_after_copy_fallback() {
        let temp =
            std::env::temp_dir().join(format!("agent-storage-paths-test-{}", uuid::Uuid::new_v4()));
        let canonical = super::AppPaths::under_root(temp.join("canonical"));
        let legacy = super::AppPaths::under_root(temp.join("legacy"));

        canonical.ensure().unwrap();
        legacy.ensure().unwrap();
        std::fs::write(&legacy.config_path, br#"{"version":1}"#).unwrap();

        let first = canonical
            .migrate_legacy_candidates(vec![legacy.clone()], None)
            .unwrap()
            .expect("expected first migration run to record migrated paths");
        assert!(
            !first.copied_paths.is_empty()
                || !first.moved_paths.is_empty()
                || !first.skipped_existing.is_empty()
        );
        assert!(legacy.config_path.exists());
        assert!(canonical.config_path.exists());

        let stored = super::load_migration_record(&canonical.migration_path)
            .unwrap()
            .expect("expected migration record to be persisted");
        assert_eq!(stored, first);

        let second = canonical
            .migrate_legacy_candidates(vec![legacy], Some(&stored))
            .unwrap();
        assert!(second.is_none());

        let stored_again = super::load_migration_record(&canonical.migration_path)
            .unwrap()
            .expect("expected migration record to remain after second run");
        assert_eq!(stored_again, first);

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn ensure_rejects_directories_outside_root() {
        let temp =
            std::env::temp_dir().join(format!("agent-storage-paths-test-{}", uuid::Uuid::new_v4()));
        let mut paths = super::AppPaths::under_root(temp.join("canonical"));
        paths.log_dir = temp.join("escape-logs");

        let error = paths.ensure().unwrap_err();

        assert!(error.to_string().contains("escapes managed root"));
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn reset_all_rejects_directories_outside_root() {
        let temp =
            std::env::temp_dir().join(format!("agent-storage-paths-test-{}", uuid::Uuid::new_v4()));
        let root = temp.join("canonical");
        let escape = temp.join("escape-data");
        let mut paths = super::AppPaths::under_root(&root);
        paths.ensure().unwrap();
        std::fs::create_dir_all(&escape).unwrap();
        paths.data_dir = escape;
        let storage = super::Storage { paths };

        let error = storage.reset_all().unwrap_err();

        assert!(error.to_string().contains("escapes managed root"));
        let _ = std::fs::remove_dir_all(&temp);
    }
}

impl Storage {
    pub fn open() -> Result<Self> {
        Self::open_with_paths(AppPaths::discover()?)
    }

    pub fn open_at(root_dir: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_paths(AppPaths::under_root(root_dir))
    }

    pub fn open_with_paths(paths: AppPaths) -> Result<Self> {
        paths.migrate_legacy_state()?;
        paths.ensure()?;
        let storage = Self { paths };
        storage.init_schema()?;
        if !storage.paths.validated_config_path()?.exists() {
            storage.save_config(&AppConfig::default())?;
        } else {
            let mut config = storage.load_config()?;
            if config.version != CONFIG_VERSION {
                config.version = CONFIG_VERSION;
                storage.save_config(&config)?;
            }
        }
        Ok(storage)
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    pub fn load_config(&self) -> Result<AppConfig> {
        let config_path = self.paths.validated_config_path()?;
        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?;
        let config =
            serde_json::from_str::<AppConfig>(&content).context("failed to parse config")?;
        Ok(config)
    }

    pub fn save_config(&self, config: &AppConfig) -> Result<()> {
        let content = serde_json::to_string_pretty(config).context("failed to serialize config")?;
        let config_path = self.paths.validated_config_path()?;
        write_atomic(&config_path, content.as_bytes())?;
        Ok(())
    }

    pub fn reset_all(&self) -> Result<()> {
        let config_dir = self.paths.validated_config_dir()?;
        let data_dir = self.paths.validated_data_dir()?;
        let log_dir = self.paths.validated_log_dir()?;

        if config_dir.exists() {
            fs::remove_dir_all(&config_dir).with_context(|| {
                format!("failed to remove config directory {}", config_dir.display())
            })?;
        }
        if data_dir.exists() {
            fs::remove_dir_all(&data_dir).with_context(|| {
                format!("failed to remove data directory {}", data_dir.display())
            })?;
        }
        if log_dir.exists() {
            fs::remove_dir_all(&log_dir)
                .with_context(|| format!("failed to remove log directory {}", log_dir.display()))?;
        }

        self.paths.ensure()?;
        self.init_schema()?;
        self.save_config(&AppConfig::default())?;
        Ok(())
    }

    pub fn sync_autostart(&self, daemon_path: &Path, args: &[&str], enabled: bool) -> Result<()> {
        let daemon = daemon_path
            .to_str()
            .ok_or_else(|| anyhow!("daemon path contains non-utf8 characters"))?;
        let mut builder = AutoLaunchBuilder::new();
        builder.set_app_name(APP_SLUG).set_app_path(daemon);
        if !args.is_empty() {
            builder.set_args(args);
        }
        let launcher = builder
            .build()
            .context("failed to construct auto-launch configuration")?;

        if enabled {
            launcher.enable().context("failed to enable auto-start")?;
        } else if launcher.is_enabled().unwrap_or(false) {
            launcher.disable().context("failed to disable auto-start")?;
        }

        Ok(())
    }

    pub fn autostart_enabled(&self, daemon_path: &Path, args: &[&str]) -> Result<bool> {
        let daemon = daemon_path
            .to_str()
            .ok_or_else(|| anyhow!("daemon path contains non-utf8 characters"))?;
        let mut builder = AutoLaunchBuilder::new();
        builder.set_app_name(APP_SLUG).set_app_path(daemon);
        if !args.is_empty() {
            builder.set_args(args);
        }
        let launcher = builder
            .build()
            .context("failed to construct auto-launch configuration")?;

        launcher
            .is_enabled()
            .context("failed to query auto-start state")
    }
}
