use super::*;

pub const MCP_DEFAULT_PORT: u16 = 47777;

/// The port Subsonic clients default to. bae's Subsonic server binds it unless
/// the user picks another.
pub const SUBSONIC_DEFAULT_PORT: u16 = 4533;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpConfig {
    pub enabled: bool,
    pub port: u16,
}

/// The single login the Subsonic/OpenSubsonic server (bae-subsonic) accepts, as
/// the server's auth uses it at request time. A third-party client authenticates
/// with `username` plus a salted-token derivation of `password`
/// (`t = md5(password + salt)`); the server checks the supplied username and
/// token against this credential. Empty strings mean no credential is
/// configured — no client can authenticate.
///
/// This is the *runtime* credential, assembled by the server controller from the
/// on-disk [`SubsonicConfig`] username plus the keyring password; it is not
/// stored on disk itself (the password never touches config).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubsonicCredential {
    pub username: String,
    pub password: String,
}

impl SubsonicCredential {
    /// The unset credential: empty username and password. No client login
    /// matches it, so the server rejects every request until one is configured.
    pub fn empty() -> Self {
        Self {
            username: String::new(),
            password: String::new(),
        }
    }
}

/// On-disk Subsonic server settings. The password is keyring-only (like the MCP
/// bearer token), so it is not here; only the non-secret `enabled`, `port`,
/// `username`, and `bind_address` persist. The server controller combines this
/// `username` with the keyring password into the runtime [`SubsonicCredential`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubsonicConfig {
    pub enabled: bool,
    pub port: u16,
    pub username: String,
    /// The IP address the server binds. `127.0.0.1` (the default) keeps it on
    /// this machine; `0.0.0.0` opens it to other devices on the network. Stored
    /// as a string but validated to parse as an [`std::net::IpAddr`].
    pub bind_address: String,
}

impl SubsonicConfig {
    pub fn disabled_default() -> Self {
        Self {
            enabled: false,
            port: SUBSONIC_DEFAULT_PORT,
            username: String::new(),
            bind_address: "127.0.0.1".to_string(),
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.port == 0 {
            return Err(ConfigError::Config(
                "Subsonic port must be between 1 and 65535".to_string(),
            ));
        }
        // An enabled server with no username can authenticate no client — the
        // salted-token check compares against this username. Refuse to enable a
        // server that would reject every login.
        if self.enabled && self.username.is_empty() {
            return Err(ConfigError::Config(
                "Subsonic server requires a username when enabled".to_string(),
            ));
        }
        if self.bind_address.parse::<std::net::IpAddr>().is_err() {
            return Err(ConfigError::Config(format!(
                "Subsonic bind address {:?} is not a valid IP address",
                self.bind_address
            )));
        }
        Ok(())
    }
}

impl McpConfig {
    pub fn disabled_default() -> Self {
        Self {
            enabled: false,
            port: MCP_DEFAULT_PORT,
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.port == 0 {
            return Err(ConfigError::Config(
                "MCP port must be between 1 and 65535".to_string(),
            ));
        }
        Ok(())
    }
}
