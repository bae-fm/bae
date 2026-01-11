use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use thiserror::Error;
use tracing::{info, warn};

/// Configuration errors (production mode only)
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Keyring error: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

fn default_true() -> bool {
    true
}

/// YAML config file structure for non-secret settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigYaml {
    pub library_id: Option<String>,
    pub torrent_bind_interface: Option<String>,
    /// Listening port for incoming torrent connections. None = random port.
    pub torrent_listen_port: Option<u16>,
    /// Enable UPnP port forwarding
    #[serde(default = "default_true")]
    pub torrent_enable_upnp: bool,
    /// Enable NAT-PMP port forwarding
    #[serde(default = "default_true")]
    pub torrent_enable_natpmp: bool,
    /// Global max connections. None = disabled/unlimited.
    pub torrent_max_connections: Option<i32>,
    /// Max connections per torrent. None = disabled/unlimited.
    pub torrent_max_connections_per_torrent: Option<i32>,
    /// Global max upload slots. None = disabled/unlimited.
    pub torrent_max_uploads: Option<i32>,
    /// Max upload slots per torrent. None = disabled/unlimited.
    pub torrent_max_uploads_per_torrent: Option<i32>,
    /// Enable the Subsonic API server
    #[serde(default = "default_true")]
    pub subsonic_enabled: bool,
    /// Subsonic server port
    pub subsonic_port: Option<u16>,
}

/// Application configuration
#[derive(Clone, Debug)]
pub struct Config {
    pub library_id: String,
    /// Discogs API key - loaded lazily from keyring when needed
    pub discogs_api_key: Option<String>,
    /// Encryption key - loaded lazily from keyring when needed (when creating encrypted storage profile)
    pub encryption_key: Option<String>,
    pub torrent_bind_interface: Option<String>,
    pub torrent_listen_port: Option<u16>,
    pub torrent_enable_upnp: bool,
    pub torrent_enable_natpmp: bool,
    pub torrent_max_connections: Option<i32>,
    pub torrent_max_connections_per_torrent: Option<i32>,
    pub torrent_max_uploads: Option<i32>,
    pub torrent_max_uploads_per_torrent: Option<i32>,
    pub subsonic_enabled: bool,
    pub subsonic_port: u16,
}

impl Config {
    pub fn load() -> Self {
        let dev_mode = std::env::var("BAE_DEV_MODE").is_ok() || dotenvy::dotenv().is_ok();
        if dev_mode {
            info!("Dev mode activated - loading from .env");
            Self::from_env()
        } else {
            info!("Production mode - loading from config.yaml");
            Self::from_config_file()
        }
    }

    fn from_env() -> Self {
        let library_id = std::env::var("BAE_LIBRARY_ID").unwrap_or_else(|_| {
            let id = uuid::Uuid::new_v4().to_string();
            warn!("No BAE_LIBRARY_ID in .env, generated new ID: {}", id);
            id
        });
        // Load from env if present, otherwise will be loaded lazily from keyring
        let discogs_api_key = std::env::var("BAE_DISCOGS_API_KEY").ok();
        let encryption_key = std::env::var("BAE_ENCRYPTION_KEY").ok();
        let torrent_bind_interface = std::env::var("BAE_TORRENT_BIND_INTERFACE")
            .ok()
            .filter(|s| !s.is_empty());

        Self {
            library_id,
            discogs_api_key,
            encryption_key,
            torrent_bind_interface,
            torrent_listen_port: None,
            torrent_enable_upnp: true,
            torrent_enable_natpmp: true,
            torrent_max_connections: None,
            torrent_max_connections_per_torrent: None,
            torrent_max_uploads: None,
            torrent_max_uploads_per_torrent: None,
            subsonic_enabled: true,
            subsonic_port: 4533,
        }
    }

    fn from_config_file() -> Self {
        // Don't load from keyring on startup - credentials loaded lazily when needed
        let home_dir = dirs::home_dir().expect("Failed to get home directory");
        let config_path = home_dir.join(".bae").join("config.yaml");
        let yaml_config: ConfigYaml = if config_path.exists() {
            serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap())
                .unwrap_or_default()
        } else {
            ConfigYaml::default()
        };

        let library_id = yaml_config
            .library_id
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        Self {
            library_id,
            discogs_api_key: None,
            encryption_key: None,
            torrent_bind_interface: yaml_config.torrent_bind_interface,
            torrent_listen_port: yaml_config.torrent_listen_port,
            torrent_enable_upnp: yaml_config.torrent_enable_upnp,
            torrent_enable_natpmp: yaml_config.torrent_enable_natpmp,
            torrent_max_connections: yaml_config.torrent_max_connections,
            torrent_max_connections_per_torrent: yaml_config.torrent_max_connections_per_torrent,
            torrent_max_uploads: yaml_config.torrent_max_uploads,
            torrent_max_uploads_per_torrent: yaml_config.torrent_max_uploads_per_torrent,
            subsonic_enabled: yaml_config.subsonic_enabled,
            subsonic_port: yaml_config.subsonic_port.unwrap_or(4533),
        }
    }

    pub fn get_library_path(&self) -> PathBuf {
        std::env::var("BAE_LIBRARY_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".bae"))
    }

    pub fn is_dev_mode() -> bool {
        std::env::var("BAE_DEV_MODE").is_ok() || std::path::Path::new(".env").exists()
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        if Self::is_dev_mode() {
            self.save_to_env()
        } else {
            self.save_to_keyring()?;
            self.save_to_config_yaml()
        }
    }

    pub fn save_to_env(&self) -> Result<(), ConfigError> {
        let env_path = std::path::Path::new(".env");
        let mut lines: Vec<String> = if env_path.exists() {
            std::io::BufReader::new(std::fs::File::open(env_path)?)
                .lines()
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };

        let mut new_values = std::collections::HashMap::new();
        new_values.insert("BAE_LIBRARY_ID", self.library_id.clone());
        if let Some(key) = &self.discogs_api_key {
            new_values.insert("BAE_DISCOGS_API_KEY", key.clone());
        }
        if let Some(key) = &self.encryption_key {
            new_values.insert("BAE_ENCRYPTION_KEY", key.clone());
        }
        if let Some(iface) = &self.torrent_bind_interface {
            new_values.insert("BAE_TORRENT_BIND_INTERFACE", iface.clone());
        }

        let mut found = std::collections::HashSet::new();
        for line in &mut lines {
            if let Some(eq) = line.find('=') {
                let key = line[..eq].trim().to_string();
                if let Some(val) = new_values.get(key.as_str()) {
                    *line = format!("{}={}", key, val);
                    found.insert(key);
                }
            }
        }
        for (key, val) in &new_values {
            if !found.contains(*key) {
                lines.push(format!("{}={}", key, val));
            }
        }
        let mut file = std::fs::File::create(env_path)?;
        for line in lines {
            writeln!(file, "{}", line)?;
        }
        Ok(())
    }

    pub fn save_to_keyring(&self) -> Result<(), ConfigError> {
        if let Some(key) = &self.discogs_api_key {
            keyring::Entry::new("bae", "discogs_api_key")?.set_password(key)?;
        }
        if let Some(key) = &self.encryption_key {
            keyring::Entry::new("bae", "encryption_master_key")?.set_password(key)?;
        }
        Ok(())
    }

    pub fn save_to_config_yaml(&self) -> Result<(), ConfigError> {
        let config_dir = self.get_library_path();
        std::fs::create_dir_all(&config_dir)?;
        let yaml = ConfigYaml {
            library_id: Some(self.library_id.clone()),
            torrent_bind_interface: self.torrent_bind_interface.clone(),
            torrent_listen_port: self.torrent_listen_port,
            torrent_enable_upnp: self.torrent_enable_upnp,
            torrent_enable_natpmp: self.torrent_enable_natpmp,
            torrent_max_connections: self.torrent_max_connections,
            torrent_max_connections_per_torrent: self.torrent_max_connections_per_torrent,
            torrent_max_uploads: self.torrent_max_uploads,
            torrent_max_uploads_per_torrent: self.torrent_max_uploads_per_torrent,
            subsonic_enabled: self.subsonic_enabled,
            subsonic_port: Some(self.subsonic_port),
        };
        std::fs::write(
            config_dir.join("config.yaml"),
            serde_yaml::to_string(&yaml).unwrap(),
        )?;
        Ok(())
    }

    /// Load discogs API key from keyring (call when Discogs API is needed)
    pub fn load_discogs_key(&mut self) {
        if self.discogs_api_key.is_none() {
            self.discogs_api_key = keyring::Entry::new("bae", "discogs_api_key")
                .ok()
                .and_then(|e| e.get_password().ok());
        }
    }

    /// Load or create encryption key from keyring (call when creating encrypted storage profile)
    pub fn load_or_create_encryption_key(&mut self) {
        if self.encryption_key.is_none() {
            // Try to load existing key from keyring
            let existing = keyring::Entry::new("bae", "encryption_master_key")
                .ok()
                .and_then(|e| e.get_password().ok());

            self.encryption_key = Some(existing.unwrap_or_else(|| {
                // Generate new key and save to keyring
                let key_hex = hex::encode(crate::encryption::generate_random_key());
                if let Ok(entry) = keyring::Entry::new("bae", "encryption_master_key") {
                    let _ = entry.set_password(&key_hex);
                }
                key_hex
            }));
        }
    }
}
