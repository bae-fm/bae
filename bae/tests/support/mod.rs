/// Initialize tracing for tests with proper test output handling
#[allow(dead_code)]
pub fn tracing_init() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true)
        .try_init();
}

/// Create a test encryption service with a zero key
pub fn test_encryption_service() -> bae::encryption::EncryptionService {
    bae::encryption::EncryptionService::new_with_key(&[0u8; 32])
}
