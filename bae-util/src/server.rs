//! The lifecycle of an embedded server the app switches on and off from
//! preferences: bind, serve, stop, and restart when the config that identifies
//! it changes, plus the status the UI reads.
//!
//! `bae-mcp` and `bae-subsonic` both run one axum server this way. What differs
//! between them is the config fields a change must restart for (the *identity*)
//! and how each builds its serve future — so those are the parameters here.
//! The errors they report are shared. Everything else — the state machine, the
//! locking, the graceful-shutdown token, the task that records a mid-flight
//! server failure — lives once. A serve future that panics is a bug: it goes
//! to the bug sink the server's owner hands the controller, and the status
//! moves to `Error` like any other failure.

use std::fmt::Display;
use std::future::{Future, IntoFuture};
use std::sync::Arc;

use tokio::sync::Mutex;

use coven::task::{spawn_owned, BugSinkRef, OwnedTask};
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
        task: OwnedTask,
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
#[derive(Clone)]
pub struct ServerController<I> {
    /// The service's name in log messages ("MCP", "Subsonic"), and the
    /// operation a panic of its serve future is reported as.
    name: &'static str,
    inner: Arc<Mutex<ServerState<I>>>,
    /// Where a panic of the serve future goes: the owner's crash reporter.
    bugs: BugSinkRef,
    /// Woken when a serve future's ending moves the state, for a test.
    #[cfg(test)]
    ended: Arc<tokio::sync::Notify>,
}

impl<I> ServerController<I>
where
    I: Clone + PartialEq + Send + 'static,
{
    pub fn new(name: &'static str, bugs: BugSinkRef) -> Self {
        Self {
            name,
            inner: Arc::new(Mutex::new(ServerState::Disabled)),
            bugs,
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
            task.ended().await;
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
        // Held from before the task exists until `Running` is stored: a serve
        // future that fails at once records its stop through the same lock, so
        // the stop lands after `Running` and replaces it, never the reverse.
        // Nothing awaits while it is held.
        let mut state = self.inner.lock().await;
        let cancellation = CancellationToken::new();
        let serving = serve(&cancellation).into_future();

        let name = self.name;
        let stopped = Stopped {
            state: self.inner.clone(),
            identity: identity.clone(),
            #[cfg(test)]
            ended: self.ended.clone(),
        };
        let panicked = stopped.clone();
        let task = spawn_owned(
            name,
            self.bugs.clone(),
            async move {
                if let Err(e) = serving.await {
                    let error = ServerError::ServerFailed {
                        detail: format!("{name} server stopped with error: {e}"),
                    };
                    warn!("{}", error.detail());
                    stopped.record(error).await;
                }
            },
            move |bug| async move {
                panicked
                    .record(ServerError::ServerFailed {
                        detail: format!("{name} server stopped: {bug}"),
                    })
                    .await;
            },
        );

        let status = ServerStatus::Running { url: url.clone() };
        *state = ServerState::Running { identity, url, cancellation, task };
        status
    }
}

/// How a serve future that stopped records it: the controller's state, and
/// the config the server was started for.
struct Stopped<I> {
    state: Arc<Mutex<ServerState<I>>>,
    identity: I,
    /// Woken once the state is moved, for a test.
    #[cfg(test)]
    ended: Arc<tokio::sync::Notify>,
}

impl<I: Clone> Clone for Stopped<I> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            identity: self.identity.clone(),
            #[cfg(test)]
            ended: self.ended.clone(),
        }
    }
}

impl<I: PartialEq> Stopped<I> {
    /// Move the controller into `Error` with `error`, unless a different
    /// server has since taken this one's place.
    async fn record(self, error: ServerError) {
        let mut state = self.state.lock().await;
        if let ServerState::Running { identity: running, .. } = &*state {
            if *running == self.identity {
                *state = ServerState::Error { error };
            }
        }
        #[cfg(test)]
        self.ended.notify_waiters();
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
