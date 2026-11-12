//! Wires the Subsonic server onto [`bae_util::server::ServerController`], which
//! runs the shared lifecycle: start, stop, and restart as the configuration
//! changes. What is Subsonic's own is here: working out from what the person
//! set, the saved password and the networks the machine is on whether the
//! server can run, where it listens, and why not when it can't.
//!
//! The person's settings are kept whatever the server can do with them. This
//! controller follows them: it starts the server as soon as nothing stands in
//! its way, moves it when its network's address changes, and stops it when
//! the network is gone. The username and the keyring password are the
//! credential the running server checks each sign-in against, so changing
//! either needs no restart.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use bae_app::AppServices;
use bae_diagnostics::Diagnostics;
use bae_preferences::{SubsonicConfig, SubsonicCredential, SubsonicNetwork};
use bae_util::server::{ServerController, ServerError, ServerStatus};
use tokio::sync::{watch, Mutex};

use crate::network::{NetworkAddress, NetworkLister};
use crate::status::{
    NetworkChoice, NotRunning, Plan, Reach, Serving, Stage, SubsonicView, SubsonicWatch,
};

/// Reads the Subsonic password from the keyring. `Ok(None)` means no password is
/// stored (an unconfigured server); `Err` is a real keyring failure.
/// Async, because a keychain read can wait on the system.
type SubsonicPasswordProvider = dyn Fn() -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<String>, String>> + Send>,
    > + Send
    + Sync;

/// The address and port a running server is bound to. A different one must
/// restart the server.
#[derive(Clone, PartialEq)]
struct Binding {
    address: Ipv4Addr,
    port: u16,
}

/// What the controller follows: the settings last applied, and the password
/// as last read.
struct Inputs {
    config: SubsonicConfig,
    password: Result<Option<String>, String>,
}

#[derive(Clone)]
pub struct SubsonicServerController {
    services: AppServices,
    password_provider: Arc<SubsonicPasswordProvider>,
    networks: Arc<NetworkLister>,
    server: ServerController<Binding>,
    /// What the settings, password and networks add up to now; held while
    /// they are worked out, so one change is carried through before the next.
    inputs: Arc<Mutex<Inputs>>,
    plan: Arc<watch::Sender<Plan>>,
    /// The credential the running server checks each sign-in against.
    credential: Arc<watch::Sender<SubsonicCredential>>,
    /// Where a panic in the server, a request or a stream is reported.
    diagnostics: Diagnostics,
}

impl SubsonicServerController {
    /// A controller with the server off, until [`Self::apply_config`] gives it
    /// the person's settings.
    pub fn new(
        services: AppServices,
        password_provider: Arc<SubsonicPasswordProvider>,
        networks: Arc<NetworkLister>,
        diagnostics: Diagnostics,
    ) -> Self {
        Self {
            services,
            password_provider,
            networks,
            server: ServerController::new("Subsonic", diagnostics.bug_sink()),
            inputs: Arc::new(Mutex::new(Inputs {
                config: SubsonicConfig::disabled_default(),
                password: Ok(None),
            })),
            plan: Arc::new(watch::Sender::new(Plan::off())),
            credential: Arc::new(watch::Sender::new(SubsonicCredential::empty())),
            diagnostics,
        }
    }

    /// Follow `config`, the settings as the person set them now: the server
    /// starts, moves or stops to match, and the password is read again.
    pub async fn apply_config(&self, config: SubsonicConfig) {
        let mut inputs = self.inputs.lock().await;
        inputs.config = config;
        inputs.password = (self.password_provider)().await;
        self.follow(&inputs).await;
    }

    /// The person's settings couldn't be read, so what to serve is unknown:
    /// the server stops, and Settings says why instead of showing it off.
    pub async fn settings_unreadable(&self, detail: String) {
        let _inputs = self.inputs.lock().await;
        self.server.shutdown().await;
        self.plan
            .send_modify(|plan| plan.stage = Stage::Blocked(vec![NotRunning::Failed { detail }]));
    }

    /// The password in the keyring changed: read it again. A running server
    /// checks the next sign-in against it; one that lacked it starts.
    pub async fn password_changed(&self) {
        let mut inputs = self.inputs.lock().await;
        inputs.password = (self.password_provider)().await;
        self.follow(&inputs).await;
    }

    /// An address on this machine came, changed or went: read the networks
    /// again and move, stop or start the server as its network requires.
    pub async fn networks_changed(&self) {
        let inputs = self.inputs.lock().await;
        self.follow(&inputs).await;
    }

    /// The server as Settings shows it, now and as it changes.
    pub fn watch(&self) -> SubsonicWatch {
        SubsonicWatch::new(self.plan.subscribe(), self.server.follow())
    }

    /// The server as Settings shows it, once it has settled.
    pub async fn view(&self) -> SubsonicView {
        self.watch().next().await.expect("the controller outlives its own view")
    }

    /// Stop the server for good: the app is closing.
    pub async fn shutdown(&self) {
        let mut inputs = self.inputs.lock().await;
        inputs.config = SubsonicConfig::disabled_default();
        self.server.shutdown().await;
        self.plan.send_replace(Plan::off());
    }

    /// Bring the server and what is told of it in line with `inputs` and the
    /// networks as they are now.
    async fn follow(&self, inputs: &Inputs) {
        let config = &inputs.config;
        let (listed, listing_failure) = match (self.networks)() {
            Ok(listed) => (listed, None),
            Err(detail) => (Vec::new(), Some(detail)),
        };
        let password = inputs.password.as_ref().ok().and_then(|p| p.as_deref()).unwrap_or("");
        self.credential.send_if_modified(|current| {
            let credential = SubsonicCredential {
                username: config.username.clone(),
                password: password.to_string(),
            };
            let changed = *current != credential;
            if changed {
                *current = credential;
            }
            changed
        });

        let stage = if !config.enabled {
            self.server.shutdown().await;
            Stage::Off
        } else {
            match self.serving(inputs, &listed, listing_failure) {
                Ok(serving) => {
                    let binding = Binding { address: serving.bind_address(), port: serving.port };
                    self.server.apply(binding, |binding| self.start(binding)).await;
                    Stage::Serving(serving)
                }
                Err(causes) => {
                    self.server.shutdown().await;
                    Stage::Blocked(causes)
                }
            }
        };
        self.plan.send_replace(Plan {
            stage,
            networks: networks_to_choose(&config.network, &listed),
            password_saved: !password.is_empty(),
        });
    }

    /// What to serve, or everything that stands in the way of serving.
    fn serving(
        &self,
        inputs: &Inputs,
        listed: &[NetworkAddress],
        listing_failure: Option<String>,
    ) -> Result<Serving, Vec<NotRunning>> {
        let config = &inputs.config;
        let mut causes = Vec::new();
        if config.username.is_empty() {
            causes.push(NotRunning::MissingUsername);
        }
        match &inputs.password {
            Ok(Some(password)) if !password.is_empty() => {}
            Ok(_) => causes.push(NotRunning::MissingPassword),
            Err(detail) => causes.push(NotRunning::Failed { detail: detail.clone() }),
        }
        let port = u16::try_from(config.port).ok().filter(|port| *port != 0);
        if port.is_none() {
            causes.push(NotRunning::PortOutOfRange);
        }
        let reach = match (&config.network, listing_failure) {
            (SubsonicNetwork::ThisMachine, _) => Some(Reach::ThisMachine),
            (SubsonicNetwork::Every | SubsonicNetwork::Interface(_), Some(detail)) => {
                causes.push(NotRunning::Failed { detail });
                None
            }
            (SubsonicNetwork::Every, None) => Some(Reach::Every(listed.to_vec())),
            (SubsonicNetwork::Interface(interface), None) => {
                let found = listed.iter().find(|network| network.interface == *interface);
                if found.is_none() {
                    causes.push(NotRunning::NetworkGone { interface: interface.clone() });
                }
                found.cloned().map(Reach::Network)
            }
        };
        match (port, reach) {
            (Some(port), Some(reach)) if causes.is_empty() => Ok(Serving { port, reach }),
            _ => Err(causes),
        }
    }

    async fn start(&self, binding: Binding) -> ServerStatus {
        let addr = SocketAddr::new(binding.address.into(), binding.port);
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => listener,
            Err(e) => {
                return self.server.record_error(ServerError::from_bind(addr, &e)).await;
            }
        };

        let url = format!("http://{}:{}/rest", binding.address, binding.port);
        let router = crate::router_following(
            self.services.clone(),
            self.credential.subscribe(),
            self.diagnostics.clone(),
        );
        self.server
            .start(binding, url, |cancellation| {
                let shutdown = cancellation.clone();
                axum::serve(listener, router)
                    .with_graceful_shutdown(async move { shutdown.cancelled_owned().await })
            })
            .await
    }
}

/// The networks to choose from: one per interface that has an address, and
/// the interface chosen even when it has none.
fn networks_to_choose(chosen: &SubsonicNetwork, listed: &[NetworkAddress]) -> Vec<NetworkChoice> {
    let mut choices: Vec<NetworkChoice> = Vec::new();
    for network in listed {
        if !choices.iter().any(|choice| choice.interface == network.interface) {
            choices.push(NetworkChoice {
                interface: network.interface.clone(),
                address: Some(network.address),
            });
        }
    }
    if let SubsonicNetwork::Interface(interface) = chosen {
        if !choices.iter().any(|choice| choice.interface == *interface) {
            choices.push(NetworkChoice { interface: interface.clone(), address: None });
            choices.sort_by(|a, b| a.interface.cmp(&b.interface));
        }
    }
    choices
}

#[cfg(test)]
#[path = "controller_tests.rs"]
mod tests;
