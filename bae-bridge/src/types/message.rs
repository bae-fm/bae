/// A `core.*` catalog message with the text its placeholders take. The UI
/// resolves the key against its string catalog and formats the arguments in.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMessage {
    pub key: String,
    /// The message's arguments in the order its source text names them:
    /// Apple's catalog takes them by that position, Android's by name.
    pub args: Vec<BridgeMessageArg>,
}

/// One named argument of a catalog message.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMessageArg {
    pub name: String,
    pub value: String,
}

impl BridgeMessageArg {
    pub(crate) fn new(name: &str, value: String) -> Self {
        Self { name: name.to_string(), value }
    }
}

/// A line that counts down to the moment `until`, such as when the next side
/// starts or when sync resumes. The UI words the catalog message `key` with
/// the whole seconds left, rounded up, as its `seconds` argument, and words it
/// again as each second passes; core, not the UI, acts at `until`, so every UI
/// shows the same number.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCountdown {
    pub key: String,
    pub until: std::time::SystemTime,
}
