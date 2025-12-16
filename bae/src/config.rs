use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use tracing::{info, warn};

use thiserror::Error;

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
/// In debug builds: loads from .env file
/// In release builds: loads from ~/.bae/config.yaml + keyring
#[derive(Clone, Debug)]
pub struct Config {
    /// Library ID (loaded from config or auto-generated)
    pub library_id: String,
    /// Discogs API key (required)
    pub discogs_api_key: String,
    /// S3 configuration
    pub s3_config: crate::cloud_storage::S3Config,
    /// Encryption key (hex-encoded 256-bit key)
    pub encryption_key: String,
    /// Number of parallel encryption workers for import (CPU-bound)
    pub max_import_encrypt_workers: usize,
    /// Number of parallel upload workers for import (I/O-bound)
    pub max_import_upload_workers: usize,
    /// Number of parallel DB write workers for import (I/O-bound)
    pub max_import_db_write_workers: usize,
    /// Size of each chunk in bytes (default: 1MB)
    pub chunk_size_bytes: usize,
    /// Network interface to bind torrent clients to (optional, e.g. "eth0", "tun0", "0.0.0.0:6881")
    pub torrent_bind_interface: Option<String>,
}

/// Credential data loaded from keyring (production mode only)
#[derive(Debug, Clone)]
struct CredentialData {
    discogs_api_key: String,
    s3_config: crate::cloud_storage::S3Config,
    encryption_key: String,
}

impl Config {
    /// Load configuration based on build mode
    /// Dev mode is activated if .env file exists or BAE_DEV_MODE env var is set
    pub fn load() -> Self {
        // Check for dev mode: .env file exists or BAE_DEV_MODE env var is set
        let dev_mode = std::env::var("BAE_DEV_MODE").is_ok() || dotenvy::dotenv().is_ok();

        if dev_mode {
            info!("Dev mode activated - loading from .env");
            Self::from_env()
        } else {
            info!("Production mode - loading from config.yaml");
            Self::from_config_file()
        }
    }

    /// Load configuration from environment variables (dev mode)
    fn from_env() -> Self {
        let library_id = match std::env::var("BAE_LIBRARY_ID").ok() {
            Some(id) => {
                info!("Using library ID from .env: {}", id);
                id
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                warn!("No BAE_LIBRARY_ID in .env, generated new ID: {}", id);
                info!(
                    "Add this to your .env file to persist: BAE_LIBRARY_ID={}",
                    id
                );
                id
            }
        };

        // Load credentials from environment variables
        let discogs_api_key = std::env::var("BAE_DISCOGS_API_KEY")
            .expect("BAE_DISCOGS_API_KEY must be set in .env for dev mode");

        // Build S3 config from environment variables
        let bucket_name =
            std::env::var("BAE_S3_BUCKET").expect("BAE_S3_BUCKET must be set in .env for dev mode");
        let region =
            std::env::var("BAE_S3_REGION").expect("BAE_S3_REGION must be set in .env for dev mode");
        let access_key_id = std::env::var("BAE_S3_ACCESS_KEY")
            .expect("BAE_S3_ACCESS_KEY must be set in .env for dev mode");
        let secret_access_key = std::env::var("BAE_S3_SECRET_KEY")
            .expect("BAE_S3_SECRET_KEY must be set in .env for dev mode");
        let endpoint_url = std::env::var("BAE_S3_ENDPOINT")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let s3_config = crate::cloud_storage::S3Config {
            bucket_name: bucket_name.clone(),
            region,
            access_key_id,
            secret_access_key,
            endpoint_url: endpoint_url.clone(),
        };

        let encryption_key = std::env::var("BAE_ENCRYPTION_KEY").unwrap_or_else(|_| {
            warn!("No BAE_ENCRYPTION_KEY found, generating temporary key");
            // Generate temporary key for dev
            use aes_gcm::{aead::OsRng, Aes256Gcm, KeyInit};
            let key = Aes256Gcm::generate_key(OsRng);
            hex::encode(key.as_ref() as &[u8])
        });

        // Import worker pool configuration
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
            .unwrap_or(1024 * 1024); // 1MB default

        let torrent_bind_interface = std::env::var("BAE_TORRENT_BIND_INTERFACE")
            .ok()
            .filter(|s| !s.is_empty());

        info!("Dev mode with S3 storage");
        info!("S3 bucket: {}", bucket_name);
        if let Some(endpoint) = &endpoint_url {
            info!("S3 endpoint: {}", endpoint);
        }
        info!(
            "Import worker pools - encrypt: {}, upload: {}, db_write: {}",
            max_import_encrypt_workers, max_import_upload_workers, max_import_db_write_workers
        );
        info!("Chunk size: {} bytes", chunk_size_bytes);

        Self {
            library_id,
            discogs_api_key,
            s3_config,
            encryption_key,
            chunk_size_bytes,
            max_import_encrypt_workers,
            max_import_upload_workers,
            max_import_db_write_workers,
            torrent_bind_interface,
        }
    }

    /// Load configuration from config.yaml + keyring (production mode)
    fn from_config_file() -> Self {
        info!("Production mode - loading from config.yaml + keyring");

        // Load from keyring
        let credentials = Self::load_from_keyring()
            .expect("Failed to load credentials from keyring - run setup wizard first");

        // Load from config.yaml
        let home_dir = dirs::home_dir().expect("Failed to get home directory");
        let config_path = home_dir.join(".bae").join("config.yaml");

        let yaml_config: ConfigYaml = if config_path.exists() {
            let yaml_str =
                std::fs::read_to_string(&config_path).expect("Failed to read config.yaml");
            serde_yaml::from_str(&yaml_str).expect("Failed to parse config.yaml")
        } else {
            warn!("No config.yaml found at {:?}, using defaults", config_path);
            ConfigYaml::default()
        };

        let library_id = yaml_config.library_id.unwrap_or_else(|| {
            let id = uuid::Uuid::new_v4().to_string();
            warn!("No library_id in config.yaml, generated new ID: {}", id);
            id
        });

        let default_encrypt_workers = std::thread::available_parallelism()
            .map(|n| n.get() * 2)
            .unwrap_or(4);

        Self {
            library_id,
            discogs_api_key: credentials.discogs_api_key,
            s3_config: credentials.s3_config,
            encryption_key: credentials.encryption_key,
            max_import_encrypt_workers: yaml_config
                .max_import_encrypt_workers
                .unwrap_or(default_encrypt_workers),
            max_import_upload_workers: yaml_config.max_import_upload_workers.unwrap_or(20),
            max_import_db_write_workers: yaml_config.max_import_db_write_workers.unwrap_or(10),
            chunk_size_bytes: yaml_config.chunk_size_bytes.unwrap_or(1024 * 1024),
            torrent_bind_interface: yaml_config.torrent_bind_interface,
        }
    }

    /// Get the library storage path
    pub fn get_library_path(&self) -> PathBuf {
        // Use ~/.bae/ directory for local database
        // TODO: This should be ~/.bae/libraries/{library_id}/ once we have library initialization
        let home_dir = dirs::home_dir().expect("Failed to get home directory");
        home_dir.join(".bae")
    }

    /// Check if running in dev mode
    pub fn is_dev_mode() -> bool {
        std::env::var("BAE_DEV_MODE").is_ok() || std::path::Path::new(".env").exists()
    }

    /// Save configuration - dispatches to appropriate backend based on mode
    pub fn save(&self) -> Result<(), ConfigError> {
        if Self::is_dev_mode() {
            self.save_to_env()
        } else {
            self.save_to_keyring()?;
            self.save_to_config_yaml()
        }
    }

    /// Save configuration to .env file (dev mode)
    pub fn save_to_env(&self) -> Result<(), ConfigError> {
        let env_path = std::path::Path::new(".env");

        // Read existing .env content, preserving comments and unknown keys
        let mut lines: Vec<String> = if env_path.exists() {
            let file = std::fs::File::open(env_path)?;
            std::io::BufReader::new(file)
                .lines()
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };

        // Keys we manage (in order for new files)
        let managed_keys = [
            "BAE_LIBRARY_ID",
            "BAE_DISCOGS_API_KEY",
            "BAE_S3_BUCKET",
            "BAE_S3_REGION",
            "BAE_S3_ACCESS_KEY",
            "BAE_S3_SECRET_KEY",
            "BAE_S3_ENDPOINT",
            "BAE_ENCRYPTION_KEY",
            "BAE_MAX_IMPORT_ENCRYPT_WORKERS",
            "BAE_MAX_IMPORT_UPLOAD_WORKERS",
            "BAE_MAX_IMPORT_DB_WRITE_WORKERS",
            "BAE_CHUNK_SIZE_BYTES",
            "BAE_TORRENT_BIND_INTERFACE",
        ];

        // Build new values map
        let mut new_values: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        new_values.insert("BAE_LIBRARY_ID".to_string(), self.library_id.clone());
        new_values.insert(
            "BAE_DISCOGS_API_KEY".to_string(),
            self.discogs_api_key.clone(),
        );
        new_values.insert(
            "BAE_S3_BUCKET".to_string(),
            self.s3_config.bucket_name.clone(),
        );
        new_values.insert("BAE_S3_REGION".to_string(), self.s3_config.region.clone());
        new_values.insert(
            "BAE_S3_ACCESS_KEY".to_string(),
            self.s3_config.access_key_id.clone(),
        );
        new_values.insert(
            "BAE_S3_SECRET_KEY".to_string(),
            self.s3_config.secret_access_key.clone(),
        );
        if let Some(endpoint) = &self.s3_config.endpoint_url {
            new_values.insert("BAE_S3_ENDPOINT".to_string(), endpoint.clone());
        }
        new_values.insert(
            "BAE_ENCRYPTION_KEY".to_string(),
            self.encryption_key.clone(),
        );
        new_values.insert(
            "BAE_MAX_IMPORT_ENCRYPT_WORKERS".to_string(),
            self.max_import_encrypt_workers.to_string(),
        );
        new_values.insert(
            "BAE_MAX_IMPORT_UPLOAD_WORKERS".to_string(),
            self.max_import_upload_workers.to_string(),
        );
        new_values.insert(
            "BAE_MAX_IMPORT_DB_WRITE_WORKERS".to_string(),
            self.max_import_db_write_workers.to_string(),
        );
        new_values.insert(
            "BAE_CHUNK_SIZE_BYTES".to_string(),
            self.chunk_size_bytes.to_string(),
        );
        if let Some(interface) = &self.torrent_bind_interface {
            new_values.insert("BAE_TORRENT_BIND_INTERFACE".to_string(), interface.clone());
        }

        // Track which keys we've updated
        let mut found_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Update existing lines by index
        for i in 0..lines.len() {
            let trimmed = lines[i].trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if let Some(eq_pos) = trimmed.find('=') {
                let key = trimmed[..eq_pos].trim().to_string();
                if let Some(new_value) = new_values.get(&key) {
                    lines[i] = format!("{}={}", key, new_value);
                    found_keys.insert(key);
                }
            }
        }

        // Append any keys that weren't found in the file
        for key in managed_keys {
            if !found_keys.contains(key) {
                if let Some(value) = new_values.get(key) {
                    lines.push(format!("{}={}", key, value));
                }
            }
        }

        // Write back
        let mut file = std::fs::File::create(env_path)?;
        for line in lines {
            writeln!(file, "{}", line)?;
        }

        info!("Saved configuration to .env");
        Ok(())
    }

    /// Save secrets to keyring (release mode)
    pub fn save_to_keyring(&self) -> Result<(), ConfigError> {
        use keyring::Entry;

        info!("Saving credentials to keyring...");

        // Save Discogs API key
        let entry = Entry::new("bae", "discogs_api_key")?;
        entry.set_password(&self.discogs_api_key)?;

        // Save S3 config as JSON
        let s3_json = serde_json::to_string(&self.s3_config)
            .map_err(|e| ConfigError::Serialization(e.to_string()))?;
        let entry = Entry::new("bae", "s3_config")?;
        entry.set_password(&s3_json)?;

        // Save encryption key
        let entry = Entry::new("bae", "encryption_master_key")?;
        entry.set_password(&self.encryption_key)?;

        info!("Saved credentials to keyring");
        Ok(())
    }

    /// Save non-secret config to config.yaml (release mode)
    pub fn save_to_config_yaml(&self) -> Result<(), ConfigError> {
        let config_dir = self.get_library_path();
        std::fs::create_dir_all(&config_dir)?;

        let config_path = config_dir.join("config.yaml");

        let yaml_config = ConfigYaml {
            library_id: Some(self.library_id.clone()),
            max_import_encrypt_workers: Some(self.max_import_encrypt_workers),
            max_import_upload_workers: Some(self.max_import_upload_workers),
            max_import_db_write_workers: Some(self.max_import_db_write_workers),
            chunk_size_bytes: Some(self.chunk_size_bytes),
            torrent_bind_interface: self.torrent_bind_interface.clone(),
        };

        let yaml_str = serde_yaml::to_string(&yaml_config)
            .map_err(|e| ConfigError::Serialization(e.to_string()))?;

        std::fs::write(&config_path, yaml_str)?;

        info!("Saved configuration to {:?}", config_path);
        Ok(())
    }

    /// Load credentials from keyring (production mode only)
    fn load_from_keyring() -> Result<CredentialData, ConfigError> {
        use keyring::Entry;

        info!("Loading credentials from keyring (password may be required)...");

        // Load Discogs API key (required)
        let discogs_api_key = match Entry::new("bae", "discogs_api_key") {
            Ok(entry) => match entry.get_password() {
                Ok(key) => {
                    info!("Loaded Discogs API key");
                    key
                }
                Err(keyring::Error::NoEntry) => {
                    return Err(ConfigError::Config(
                        "No Discogs API key found - run setup wizard first".to_string(),
                    ));
                }
                Err(e) => return Err(ConfigError::Keyring(e)),
            },
            Err(e) => return Err(ConfigError::Keyring(e)),
        };

        // Load S3 config (required)
        let s3_config = match Entry::new("bae", "s3_config") {
            Ok(entry) => match entry.get_password() {
                Ok(json) => {
                    let config: crate::cloud_storage::S3Config = serde_json::from_str(&json)
                        .map_err(|e| ConfigError::Serialization(e.to_string()))?;
                    info!("Loaded S3 configuration");
                    config
                }
                Err(keyring::Error::NoEntry) => {
                    return Err(ConfigError::Config(
                        "No S3 configuration found - run setup wizard first".to_string(),
                    ));
                }
                Err(e) => return Err(ConfigError::Keyring(e)),
            },
            Err(e) => return Err(ConfigError::Keyring(e)),
        };

        // Load encryption master key
        let encryption_key = match Entry::new("bae", "encryption_master_key") {
            Ok(entry) => match entry.get_password() {
                Ok(key_hex) => {
                    info!("Loaded encryption master key");
                    key_hex
                }
                Err(keyring::Error::NoEntry) => {
                    return Err(ConfigError::Config(
                        "No encryption key found - run setup wizard first".to_string(),
                    ));
                }
                Err(e) => return Err(ConfigError::Keyring(e)),
            },
            Err(e) => return Err(ConfigError::Keyring(e)),
        };

        Ok(CredentialData {
            discogs_api_key,
            s3_config,
            encryption_key,
        })
    }
}

/// Hook to access config from components (using Dioxus context)
/// The config is provided via UIContext in main.rs
pub fn use_config() -> Config {
    use dioxus::prelude::use_context;
    let context = use_context::<crate::ui::AppContext>();
    context.config
}
