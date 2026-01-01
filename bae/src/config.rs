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

/// YAML config file structure for non-secret settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigYaml {
    pub library_id: Option<String>,
    pub max_import_encrypt_workers: Option<usize>,
    pub max_import_upload_workers: Option<usize>,
    pub max_import_db_write_workers: Option<usize>,
    pub chunk_size_bytes: Option<usize>,
    pub torrent_bind_interface: Option<String>,
}

/// Application configuration
#[derive(Clone, Debug)]
pub struct Config {
    pub library_id: String,
    pub discogs_api_key: Option<String>,
    pub encryption_key: String,
    pub max_import_encrypt_workers: usize,
    pub max_import_upload_workers: usize,
    pub max_import_db_write_workers: usize,
    pub chunk_size_bytes: usize,
    pub torrent_bind_interface: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct CredentialData {
    discogs_api_key: Option<String>,
    encryption_key: Option<String>,
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
        let discogs_api_key = std::env::var("BAE_DISCOGS_API_KEY").ok();
        let encryption_key = std::env::var("BAE_ENCRYPTION_KEY").unwrap_or_else(|_| {
            use aes_gcm::{aead::OsRng, Aes256Gcm, KeyInit};
            let key = Aes256Gcm::generate_key(OsRng);
            hex::encode(key.as_ref() as &[u8])
        });
        let max_import_encrypt_workers = std::env::var("BAE_MAX_IMPORT_ENCRYPT_WORKERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|n| n.get() * 2)
                    .unwrap_or(4)
            });
        let max_import_upload_workers = std::env::var("BAE_MAX_IMPORT_UPLOAD_WORKERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(20);
        let max_import_db_write_workers = std::env::var("BAE_MAX_IMPORT_DB_WRITE_WORKERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        let chunk_size_bytes = std::env::var("BAE_CHUNK_SIZE_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1024 * 1024);
        let torrent_bind_interface = std::env::var("BAE_TORRENT_BIND_INTERFACE")
            .ok()
            .filter(|s| !s.is_empty());

        Self {
            library_id,
            discogs_api_key,
            encryption_key,
            chunk_size_bytes,
            max_import_encrypt_workers,
            max_import_upload_workers,
            max_import_db_write_workers,
            torrent_bind_interface,
        }
    }

    fn from_config_file() -> Self {
        let credentials = Self::load_from_keyring();
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
        let encryption_key = credentials.encryption_key.unwrap_or_else(|| {
            use aes_gcm::{aead::OsRng, Aes256Gcm, KeyInit};
            let key = Aes256Gcm::generate_key(OsRng);
            let key_hex = hex::encode(key.as_ref() as &[u8]);
            if let Ok(entry) = keyring::Entry::new("bae", "encryption_master_key") {
                let _ = entry.set_password(&key_hex);
            }
            key_hex
        });
        let default_workers = std::thread::available_parallelism()
            .map(|n| n.get() * 2)
            .unwrap_or(4);

        Self {
            library_id,
            discogs_api_key: credentials.discogs_api_key,
            encryption_key,
            max_import_encrypt_workers: yaml_config
                .max_import_encrypt_workers
                .unwrap_or(default_workers),
            max_import_upload_workers: yaml_config.max_import_upload_workers.unwrap_or(20),
            max_import_db_write_workers: yaml_config.max_import_db_write_workers.unwrap_or(10),
            chunk_size_bytes: yaml_config.chunk_size_bytes.unwrap_or(1024 * 1024),
            torrent_bind_interface: yaml_config.torrent_bind_interface,
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
        new_values.insert("BAE_ENCRYPTION_KEY", self.encryption_key.clone());
        new_values.insert(
            "BAE_MAX_IMPORT_ENCRYPT_WORKERS",
            self.max_import_encrypt_workers.to_string(),
        );
        new_values.insert(
            "BAE_MAX_IMPORT_UPLOAD_WORKERS",
            self.max_import_upload_workers.to_string(),
        );
        new_values.insert(
            "BAE_MAX_IMPORT_DB_WRITE_WORKERS",
            self.max_import_db_write_workers.to_string(),
        );
        new_values.insert("BAE_CHUNK_SIZE_BYTES", self.chunk_size_bytes.to_string());
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
        keyring::Entry::new("bae", "encryption_master_key")?.set_password(&self.encryption_key)?;
        Ok(())
    }

    pub fn save_to_config_yaml(&self) -> Result<(), ConfigError> {
        let config_dir = self.get_library_path();
        std::fs::create_dir_all(&config_dir)?;
        let yaml = ConfigYaml {
            library_id: Some(self.library_id.clone()),
            max_import_encrypt_workers: Some(self.max_import_encrypt_workers),
            max_import_upload_workers: Some(self.max_import_upload_workers),
            max_import_db_write_workers: Some(self.max_import_db_write_workers),
            chunk_size_bytes: Some(self.chunk_size_bytes),
            torrent_bind_interface: self.torrent_bind_interface.clone(),
        };
        std::fs::write(
            config_dir.join("config.yaml"),
            serde_yaml::to_string(&yaml).unwrap(),
        )?;
        Ok(())
    }

    fn load_from_keyring() -> CredentialData {
        CredentialData {
            discogs_api_key: keyring::Entry::new("bae", "discogs_api_key")
                .ok()
                .and_then(|e| e.get_password().ok()),
            encryption_key: keyring::Entry::new("bae", "encryption_master_key")
                .ok()
                .and_then(|e| e.get_password().ok()),
        }
    }
}

pub fn use_config() -> Config {
    dioxus::prelude::use_context::<crate::ui::AppContext>().config
}
