//! The lifecycle of an embedded server the app switches on and off from
//! preferences: bind, serve, stop, and restart when the config that identifies
//! it changes, plus the status the UI reads.
//!
//! `bae-mcp` and `bae-subsonic` both run one axum server this way. What differs
//! between them is the config fields a change must restart for (the *identity*),
//! the error vocabulary each reports, and how each builds its serve future —
//! so those are the parameters here. Everything else — the state machine, the
//! locking, the graceful-shutdown token, the task that records a mid-flight
//! server failure — lives once.

use std::fmt::Display;
use std::future::{Future, IntoFuture};
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

/// A service's own error enum. Every variant carries a human-readable detail;
/// this is how the shared lifecycle reads it without knowing the variants.
pub trait ServerError: Clone {
    fn detail(&self) -> &str;
}

/// What the app reports for a server: off, reachable at a URL, or stopped with
/// the reason it failed.
#[derive(Debug, Clone)]
pub enum ServerStatus<E> {
    Disabled,
    Running { url: String },
    Error { error: E },
}

enum State<I, E> {
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
        error: E,
    },
}

impl<I, E: Clone> State<I, E> {
    fn status(&self) -> ServerStatus<E> {
        match self {
            Self::Disabled => ServerStatus::Disabled,
            Self::Running { url, .. } => ServerStatus::Running { url: url.clone() },
            Self::Error { error } => ServerStatus::Error {
                error: error.clone(),
            },
        }
    }
}

/// Owns one server's running state. Services embed this and add their own
/// config handling around it.
#[derive(Clone)]
pub struct ServerController<I, E> {
    /// The service's name in log messages ("MCP", "Subsonic").
    name: &'static str,
    inner: Arc<Mutex<State<I, E>>>,
}

impl<I, E> ServerController<I, E>
where
    I: Clone + PartialEq + Send + 'static,
    E: ServerError + Send + 'static,
{
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            inner: Arc::new(Mutex::new(State::Disabled)),
        }
    }

    pub async fn status(&self) -> ServerStatus<E> {
        self.inner.lock().await.status()
    }

    /// Stop whatever is running and report the server off.
    pub async fn disable(&self) -> ServerStatus<E> {
        self.shutdown().await;
        ServerStatus::Disabled
    }

    /// Leave a server already bound to `identity` alone; otherwise stop what is
    /// running and hand `identity` to `start`.
    pub async fn apply<Fut>(&self, identity: I, start: impl FnOnce(I) -> Fut) -> ServerStatus<E>
    where
        Fut: Future<Output = ServerStatus<E>>,
    {
        {
            let state = self.inner.lock().await;
            if let State::Running {
                identity: running, ..
            } = &*state
            {
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
            match std::mem::replace(&mut *state, State::Disabled) {
                State::Running {
                    cancellation, task, ..
                } => {
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
    pub async fn record_error(&self, error: E) -> ServerStatus<E> {
        self.shutdown().await;
        let mut state = self.inner.lock().await;
        *state = State::Error {
            error: error.clone(),
        };
        ServerStatus::Error { error }
    }

    /// Spawn the serve future and mark the server running at `url`.
    ///
    /// `serve` receives the controller's cancellation token — the future it
    /// returns must stop when that token is cancelled, since [`Self::shutdown`]
    /// cancels it and then waits for the task. A serve future that ends with an
    /// error moves the controller into `Error` (built through `server_failed`),
    /// unless a different server has since taken its place.
    pub async fn start<S, Err>(
        &self,
        identity: I,
        url: String,
        server_failed: fn(String) -> E,
        serve: impl FnOnce(&CancellationToken) -> S,
    ) -> ServerStatus<E>
    where
        S: IntoFuture<Output = Result<(), Err>>,
        S::IntoFuture: Send + 'static,
        Err: Display + Send,
    {
        let cancellation = CancellationToken::new();
        let serving = serve(&cancellation).into_future();

        let name = self.name;
        let task_state = self.inner.clone();
        let task_identity = identity.clone();
        let task = tokio::spawn(async move {
            if let Err(e) = serving.await {
                let error = server_failed(format!("{name} server stopped with error: {e}"));
                warn!("{}", error.detail());
                let mut state = task_state.lock().await;
                if let State::Running {
                    identity: running, ..
                } = &*state
                {
                    if *running == task_identity {
                        *state = State::Error { error };
                    }
                }
            }
        });

        let status = ServerStatus::Running { url: url.clone() };
        *self.inner.lock().await = State::Running {
            identity,
            url,
            cancellation,
            task,
        };
        status
    }
}
