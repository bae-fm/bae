//! One external service's base address, redirectable in test builds.

/// Where a service's requests go: a fixed address, plus a seam that test builds
/// use to point them at a local server instead.
///
/// Production compiles to the address alone — no lock and no branch. Test and
/// `test-utils` builds hold a process-wide override, so tests that set one must
/// be serialized against each other.
pub struct TestBaseUrl {
    address: &'static str,
    #[cfg(any(test, feature = "test-utils"))]
    redirect: std::sync::Mutex<Option<String>>,
}

impl TestBaseUrl {
    pub const fn new(address: &'static str) -> Self {
        Self {
            address,
            #[cfg(any(test, feature = "test-utils"))]
            redirect: std::sync::Mutex::new(None),
        }
    }

    /// Where requests built right now go.
    pub fn get(&self) -> String {
        #[cfg(any(test, feature = "test-utils"))]
        if let Some(redirect) = self
            .redirect
            .lock()
            .expect("base URL mutex poisoned")
            .clone()
        {
            return redirect;
        }
        self.address.to_string()
    }

    /// Point every request built after this call at `url`; `None` restores the
    /// fixed address.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn set_for_test(&self, url: Option<String>) {
        *self.redirect.lock().expect("base URL mutex poisoned") = url;
    }
}
