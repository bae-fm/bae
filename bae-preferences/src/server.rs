use super::*;

pub const MCP_DEFAULT_PORT: u16 = 47777;

/// The port Subsonic clients default to. bae's Subsonic server binds it unless
/// the user picks another.
pub const SUBSONIC_DEFAULT_PORT: u32 = 4533;

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
        Self { username: String::new(), password: String::new() }
    }
}

/// Where the Subsonic server listens: on this machine only, on one network by
/// its interface, or on every network. A network is kept as its interface (the
/// BSD name, such as `en0`), never as its address, so the server follows it to
/// a new address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubsonicNetwork {
    ThisMachine,
    Interface(String),
    Every,
}

/// On-disk Subsonic server settings: what the person wants, kept as they set
/// it. The password is keyring-only (like the MCP bearer token), so it is not
/// here. Whether the server can run on these is the server's to say, not a
/// reason to refuse them: `enabled` stays on while something stands in the
/// way, and the server starts as soon as nothing does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubsonicConfig {
    pub enabled: bool,
    /// The port as typed. A value outside 1 to 65535 is kept, and the server
    /// reports that it can't run on it.
    pub port: u32,
    pub username: String,
    pub network: SubsonicNetwork,
}

impl SubsonicConfig {
    pub fn disabled_default() -> Self {
        Self {
            enabled: false,
            port: SUBSONIC_DEFAULT_PORT,
            username: String::new(),
            network: SubsonicNetwork::ThisMachine,
        }
    }
}

impl McpConfig {
    pub fn disabled_default() -> Self {
        Self { enabled: false, port: MCP_DEFAULT_PORT }
    }

    pub fn validate(&self) -> Result<(), PreferencesError> {
        if self.port == 0 {
            return Err(PreferencesError::Invalid(
                "MCP port must be between 1 and 65535".to_string(),
            ));
        }
        Ok(())
    }
}
