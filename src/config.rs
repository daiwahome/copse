use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub auto_commit: bool,
    pub auto_permissions: bool,
    pub permission_mode: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            auto_commit: false,
            auto_permissions: false,
            permission_mode: "default".to_string(),
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let cfg = confy::load("copse", None).map_err(|e| {
            let path = confy::get_configuration_file_path("copse", None)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "<unknown>".to_string());
            anyhow::anyhow!("Failed to load config from {path}: {e}")
        })?;
        Ok(cfg)
    }

    /// Write the default config file. Returns an error if the file already exists.
    pub fn init() -> anyhow::Result<()> {
        let path = confy::get_configuration_file_path("copse", None)
            .map_err(|e| anyhow::anyhow!("Failed to determine config path: {e}"))?;

        if path.exists() {
            anyhow::bail!("Config file already exists: {}", path.display());
        }

        confy::store("copse", None, Self::default())
            .map_err(|e| anyhow::anyhow!("Failed to write config: {e}"))?;

        println!("Created {}", path.display());
        Ok(())
    }
}
