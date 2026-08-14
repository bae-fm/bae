use super::*;

#[derive(Debug)]
struct ThreadRecordingCredential {
    execution_threads: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl ThreadRecordingCredential {
    fn record_execution_thread(&self) {
        self.execution_threads
            .lock()
            .expect("record keyring execution thread")
            .push(
                std::thread::current()
                    .name()
                    .unwrap_or("unnamed")
                    .to_string(),
            );
    }
}

impl keyring_core::api::CredentialApi for ThreadRecordingCredential {
    fn set_secret(&self, _secret: &[u8]) -> keyring_core::Result<()> {
        self.record_execution_thread();
        Ok(())
    }

    fn get_secret(&self) -> keyring_core::Result<Vec<u8>> {
        self.record_execution_thread();
        Ok(b"stored-secret".to_vec())
    }

    fn delete_credential(&self) -> keyring_core::Result<()> {
        self.record_execution_thread();
        Ok(())
    }

    fn get_credential(
        &self,
    ) -> keyring_core::Result<Option<std::sync::Arc<keyring_core::Credential>>> {
        Ok(None)
    }

    fn get_specifiers(&self) -> Option<(String, String)> {
        Some(("bae-test".to_string(), "account".to_string()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug)]
struct ThreadRecordingStore {
    execution_threads: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl keyring_core::api::CredentialStoreApi for ThreadRecordingStore {
    fn vendor(&self) -> String {
        "bae test keyring".to_string()
    }

    fn id(&self) -> String {
        "thread recording store".to_string()
    }

    fn build(
        &self,
        _service: &str,
        _user: &str,
        _modifiers: Option<&std::collections::HashMap<&str, &str>>,
    ) -> keyring_core::Result<keyring_core::Entry> {
        Ok(keyring_core::Entry::new_with_credential(
            std::sync::Arc::new(ThreadRecordingCredential {
                execution_threads: self.execution_threads.clone(),
            }),
        ))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[test]
fn bae_store_secrets_execute_on_covens_keyring_worker() {
    const CHILD: &str = "BAE_KEYRING_WORKER_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let execution_threads = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        keyring_core::set_default_store(std::sync::Arc::new(ThreadRecordingStore {
            execution_threads: execution_threads.clone(),
        }));
        coven::set_keyring_service("bae-keyring-worker-test").expect("register keyring service");
        let keys = StoreKeys::bind("store-1".to_string());

        keys.set_discogs_key("discogs-token")
            .expect("set Discogs key");
        keys.get_discogs_key().expect("get Discogs key");
        keys.delete_discogs_key().expect("delete Discogs key");
        keys.set_mcp_token("mcp-token").expect("set MCP token");
        keys.get_mcp_token().expect("get MCP token");
        keys.set_subsonic_password("subsonic-password")
            .expect("set Subsonic password");
        keys.get_subsonic_password().expect("get Subsonic password");
        keys.set_encryption_key("encryption-key")
            .expect("set encryption key");
        keys.forget_encryption_key().expect("forget encryption key");

        let threads = execution_threads
            .lock()
            .expect("read keyring execution threads");
        assert!(
            !threads.is_empty(),
            "the test store must observe operations"
        );
        assert!(
            threads.iter().all(|thread| thread == "coven-keyring"),
            "every keyring operation must execute on Coven's worker: {threads:?}"
        );
        return;
    }

    let status = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .arg("keys::tests::bae_store_secrets_execute_on_covens_keyring_worker")
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD, "1")
        .status()
        .expect("run keyring worker subprocess");
    assert!(
        status.success(),
        "keyring worker subprocess failed: {status}"
    );
}
