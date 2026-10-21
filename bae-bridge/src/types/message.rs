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
        Self {
            name: name.to_string(),
            value,
        }
    }
}
