use super::*;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root_dir: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub plugin_dir: PathBuf,
    pub config_path: PathBuf,
    pub db_path: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let dirs = ProjectDirs::from("com", "NuclearAI", APP_NAME)
            .ok_or_else(|| anyhow!("failed to resolve application directories"))?;

        let config_dir = dirs.config_dir().to_path_buf();
        let data_dir = dirs.data_dir().to_path_buf();
        let log_dir = dirs.data_local_dir().join("logs");
        let root_dir = dirs.data_local_dir().to_path_buf();
        let plugin_dir = data_dir.join("plugins");

        Ok(Self {
            config_path: config_dir.join("config.json"),
            db_path: data_dir.join("agent.db"),
            root_dir,
            config_dir,
            data_dir,
            log_dir,
            plugin_dir,
        })
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
            root_dir,
            config_dir,
            data_dir,
            log_dir,
            plugin_dir,
        }
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.root_dir).context("failed to create root dir")?;
        fs::create_dir_all(&self.config_dir).context("failed to create config dir")?;
        fs::create_dir_all(&self.data_dir).context("failed to create data dir")?;
        fs::create_dir_all(&self.log_dir).context("failed to create log dir")?;
        fs::create_dir_all(&self.plugin_dir).context("failed to create plugin dir")?;
        Ok(())
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
        paths.ensure()?;
        let storage = Self { paths };
        storage.init_schema()?;
        if !storage.paths.config_path.exists() {
            storage.save_config(&AppConfig::default())?;
        }
        Ok(storage)
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    pub fn load_config(&self) -> Result<AppConfig> {
        let content = fs::read_to_string(&self.paths.config_path)
            .with_context(|| format!("failed to read {}", self.paths.config_path.display()))?;
        let config =
            serde_json::from_str::<AppConfig>(&content).context("failed to parse config")?;
        Ok(config)
    }

    pub fn save_config(&self, config: &AppConfig) -> Result<()> {
        let content = serde_json::to_string_pretty(config).context("failed to serialize config")?;
        write_atomic(&self.paths.config_path, content.as_bytes())?;
        Ok(())
    }

    pub fn reset_all(&self) -> Result<()> {
        if self.paths.config_dir.exists() {
            fs::remove_dir_all(&self.paths.config_dir).with_context(|| {
                format!(
                    "failed to remove config directory {}",
                    self.paths.config_dir.display()
                )
            })?;
        }
        if self.paths.data_dir.exists() {
            fs::remove_dir_all(&self.paths.data_dir).with_context(|| {
                format!(
                    "failed to remove data directory {}",
                    self.paths.data_dir.display()
                )
            })?;
        }
        if self.paths.log_dir.exists() {
            fs::remove_dir_all(&self.paths.log_dir).with_context(|| {
                format!(
                    "failed to remove log directory {}",
                    self.paths.log_dir.display()
                )
            })?;
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
