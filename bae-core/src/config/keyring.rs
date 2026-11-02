/// Register bae's keyring namespace and let coven install the platform store
/// and policy it owns. Startup must not continue with an unusable credential
/// store; the caller reports a failure.
pub fn register_keyring_service() -> Result<(), coven::KeyError> {
    coven::set_keyring_service("bae")
}
