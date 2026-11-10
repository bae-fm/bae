//! The lifecycle of an embedded server the app switches on and off from
//! preferences: bind, serve, stop, and restart when the config that identifies
//! it changes, plus the status the UI reads.
//!
//! `bae-mcp` and `bae-subsonic` both run one axum server this way. What differs
//! between them is the config fields a change must restart for (the *identity*)
//! and how each builds its serve future — so those are the parameters here.
//! The errors they report are shared. Everything else — the state machine, the
//! locking, the graceful-shutdown token, the task that records a mid-flight
//! server failure — lives once. A serve future that panics is a bug: the
//! server's owner reports it through the `bug` it hands the controller, and
//! the status moves to `Error` like any other failure.

use std::fmt::Display;
use std::future::{Future, IntoFuture};
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::task_failure::{caught, Bug};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

/// Why a server is not running. Every variant carries a human-readable
/// detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerError {
    /// The stored config cannot be served.
    InvalidConfig { detail: String },
    /// The server's own secret could not be read from the keychain: the MCP
    /// bearer token, the Subsonic password.
    CredentialUnavailable { detail: String },
    /// The address could not be bound.
    BindFailed { detail: String },
    /// The server stopped while running.
    ServerFailed { detail: String },
}

impl ServerError {
    pub fn detail(&self) -> &str {
        match self {
            Self::InvalidConfig { detail }
            | Self::CredentialUnavailable { detail }
            | Self::BindFailed { detail }
            | Self::ServerFailed { detail } => detail,
        }
    }
}

/// What the app reports for a server: off, reachable at a URL, or stopped with
/// the reason it failed.
#[derive(Debug, Clone)]
pub enum ServerStatus {
    Disabled,
    Running { url: String },
    Error { error: ServerError },
}

enum ServerState<I> {
    Disabled,
    Running {
        /// The config the running server is bound to. A config that differs
        /// here must restart the server; one that matches is already applied.
        identity: I,
        url: String,
        cancellation: CancellationToken,
        task: JoinHandle<()>,
    },
    Error {
        error: ServerError,
    },
}

impl<I> ServerState<I> {
    fn status(&self) -> ServerStatus {
        match self {
            Self::Disabled => ServerStatus::Disabled,
            Self::Running { url, .. } => ServerStatus::Running { url: url.clone() },
            Self::Error { error } => ServerStatus::Error { error: error.clone() },
        }
    }
}

/// Owns one server's running state. Services embed this and add their own
/// config handling around it.
/// How a server's owner reports a bug — its serve future panicking — and
/// hands the failure back.
pub type ReportBug = Arc<dyn Fn(Bug) -> Bug + Send + Sync>;

#[derive(Clone)]
pub struct ServerController<I> {
    /// The service's name in log messages ("MCP", "Subsonic"), and the
    /// operation a panic of its serve future is reported as.
    name: &'static str,
    inner: Arc<Mutex<ServerState<I>>>,
    bug: ReportBug,
    /// Woken when a serve future's ending moves the state, for a test.
    #[cfg(test)]
    ended: Arc<tokio::sync::Notify>,
}

impl<I> ServerController<I>
where
    I: Clone + PartialEq + Send + 'static,
{
    pub fn new(name: &'static str, bug: ReportBug) -> Self {
        Self {
            name,
            inner: Arc::new(Mutex::new(ServerState::Disabled)),
            bug,
            #[cfg(test)]
            ended: Arc::default(),
        }
    }

    pub async fn status(&self) -> ServerStatus {
        self.inner.lock().await.status()
    }

    /// Stop whatever is running and report the server off.
    pub async fn disable(&self) -> ServerStatus {
        self.shutdown().await;
        ServerStatus::Disabled
    }

    /// Leave a server already bound to `identity` alone; otherwise stop what is
    /// running and hand `identity` to `start`.
    pub async fn apply<Fut>(&self, identity: I, start: impl FnOnce(I) -> Fut) -> ServerStatus
    where
        Fut: Future<Output = ServerStatus>,
    {
        {
            let state = self.inner.lock().await;
            if let ServerState::Running { identity: running, .. } = &*state {
                if *running == identity {
                    return state.status();
                }
            }
        }

        self.shutdown().await;
        start(identity).await
    }

    pub async fn shutdown(&self) {
        let task = {
            let mut state = self.inner.lock().await;
            match std::mem::replace(&mut *state, ServerState::Disabled) {
                ServerState::Running { cancellation, task, .. } => {
                    cancellation.cancel();
                    Some(task)
                }
                _ => None,
            }
        };
        if let Some(task) = task {
            if let Err(error) = task.await {
                warn!("{} server task join failed: {error}", self.name);
            }
        }
    }

    /// Record a failure to start: nothing runs, and the status carries why.
    pub async fn record_error(&self, error: ServerError) -> ServerStatus {
        self.shutdown().await;
        let mut state = self.inner.lock().await;
        *state = ServerState::Error { error: error.clone() };
        ServerStatus::Error { error }
    }

    /// Spawn the serve future and mark the server running at `url`.
    ///
    /// `serve` receives the controller's cancellation token — the future it
    /// returns must stop when that token is cancelled, since [`Self::shutdown`]
    /// cancels it and then waits for the task. A serve future that ends with an
    /// error, or panics, moves the controller into `Error` as
    /// [`ServerError::ServerFailed`], unless a different server has since taken its place;
    /// a panic is reported first.
    pub async fn start<S, Err>(
        &self,
        identity: I,
        url: String,
        serve: impl FnOnce(&CancellationToken) -> S,
    ) -> ServerStatus
    where
        S: IntoFuture<Output = Result<(), Err>>,
        S::IntoFuture: Send + 'static,
        Err: Display + Send,
    {
        let cancellation = CancellationToken::new();
        let serving = serve(&cancellation).into_future();

        let name = self.name;
        let bug = self.bug.clone();
        let task_state = self.inner.clone();
        let task_identity = identity.clone();
        #[cfg(test)]
        let ended = self.ended.clone();
        #[expect(
            clippy::disallowed_methods,
            reason = "the task catches its own panic and reports it through the bug function the owner handed in; bae-util is below bae-diagnostics"
        )]
        let task = tokio::spawn(async move {
            let error = match caught(name, serving).await {
                Ok(Ok(())) => return,
                Ok(Err(e)) => {
                    let error = ServerError::ServerFailed {
                        detail: format!("{name} server stopped with error: {e}"),
                    };
                    warn!("{}", error.detail());
                    error
                }
                Err(failed) => ServerError::ServerFailed {
                    detail: format!("{name} server stopped: {}", bug(failed)),
                },
            };
            let mut state = task_state.lock().await;
            if let ServerState::Running { identity: running, .. } = &*state {
                if *running == task_identity {
                    *state = ServerState::Error { error };
                }
            }
            #[cfg(test)]
            ended.notify_waiters();
        });

        let status = ServerStatus::Running { url: url.clone() };
        *self.inner.lock().await = ServerState::Running { identity, url, cancellation, task };
        status
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
