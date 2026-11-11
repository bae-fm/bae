//! What the Subsonic server is doing, as Settings and the rest of the app show
//! it: off, running at which addresses, or not running and why.

use std::net::Ipv4Addr;

use bae_util::server::{ServerError, ServerStatus, ServerWatch};
use tokio::sync::watch;

use crate::network::NetworkAddress;

/// What the Subsonic server is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubsonicStatus {
    /// The person has it switched off.
    Off,
    /// Serving, at these addresses.
    Running { addresses: Vec<ServedAddress> },
    /// Switched on and not serving, for these reasons: never none.
    NotRunning { causes: Vec<NotRunning>, tone: NotRunningTone },
}

impl SubsonicStatus {
    /// Not running for `causes`, which are never none.
    pub fn not_running(causes: Vec<NotRunning>) -> Self {
        let tone = if causes.iter().any(|cause| matches!(cause, NotRunning::Failed { .. })) {
            NotRunningTone::Danger
        } else {
            NotRunningTone::Warning
        };
        Self::NotRunning { causes, tone }
    }
}

/// How much a server that is switched on and not running needs the person:
/// a warning when something they can set stands in the way, danger when the
/// server could not start for a reason of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotRunningTone {
    Warning,
    Danger,
}

/// An address a Subsonic app connects to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedAddress {
    /// The server's address with its port, never `0.0.0.0` and without the
    /// path the API lives under: `http://192.168.1.20:4533`.
    pub url: String,
    /// The BSD name of the interface it is on; none for this machine itself.
    pub interface: Option<String>,
}

/// One reason the server is switched on and not serving.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotRunning {
    /// No username to sign in with.
    MissingUsername,
    /// No password to sign in with.
    MissingPassword,
    /// The port is not a number from 1 to 65535.
    PortOutOfRange,
    /// Another program holds the port.
    PortTaken,
    /// The network chosen has no address: it is not connected.
    NetworkGone { interface: String },
    /// Anything else: what the system said.
    Failed { detail: String },
}

/// A network the person can choose to listen on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkChoice {
    /// The interface's BSD name.
    pub interface: String,
    /// Its address; none when it is the network chosen and has none now.
    pub address: Option<Ipv4Addr>,
}

/// The Subsonic server as Settings shows it besides what the person set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubsonicView {
    pub status: SubsonicStatus,
    /// The networks to choose from, by interface name: those with an address,
    /// and the one chosen even when it has none.
    pub networks: Vec<NetworkChoice>,
    /// Whether a password is saved.
    pub password_saved: bool,
}

/// What the controller decided the server should be doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    pub(crate) stage: Stage,
    pub(crate) networks: Vec<NetworkChoice>,
    pub(crate) password_saved: bool,
}

impl Plan {
    /// Before any settings have been applied: the server is off.
    pub(crate) fn off() -> Self {
        Self { stage: Stage::Off, networks: Vec::new(), password_saved: false }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Stage {
    Off,
    /// Switched on, and something stands in the way before the server is
    /// tried.
    Blocked(Vec<NotRunning>),
    /// Switched on with nothing in the way: the server is started on this.
    Serving(Serving),
}

/// What a server being started is to serve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Serving {
    pub(crate) port: u16,
    pub(crate) reach: Reach,
}

/// Which addresses reach the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Reach {
    /// This machine only.
    ThisMachine,
    /// One network, at the address it has.
    Network(NetworkAddress),
    /// Every network the machine is on, at each of their addresses.
    Every(Vec<NetworkAddress>),
}

impl Serving {
    /// The address the server binds.
    pub(crate) fn bind_address(&self) -> Ipv4Addr {
        match &self.reach {
            Reach::ThisMachine => Ipv4Addr::LOCALHOST,
            Reach::Network(network) => network.address,
            Reach::Every(_) => Ipv4Addr::UNSPECIFIED,
        }
    }

    /// What the server reports as its URL once it runs.
    pub(crate) fn server_url(&self) -> String {
        format!("http://{}:{}/rest", self.bind_address(), self.port)
    }

    fn addresses(&self) -> Vec<ServedAddress> {
        let url = |host: &dyn std::fmt::Display| format!("http://{host}:{}", self.port);
        let this_machine = || ServedAddress { url: url(&"localhost"), interface: None };
        let on = |network: &NetworkAddress| ServedAddress {
            url: url(&network.address),
            interface: Some(network.interface.clone()),
        };
        match &self.reach {
            Reach::ThisMachine => vec![this_machine()],
            Reach::Network(network) => vec![on(network)],
            // A machine on no network is still reached at itself.
            Reach::Every(networks) if networks.is_empty() => vec![this_machine()],
            Reach::Every(networks) => networks.iter().map(on).collect(),
        }
    }
}

/// The view the plan and the server's own state make together, or none while
/// the server is between two states of a restart the plan has not caught up
/// with.
fn view(plan: &Plan, server: &ServerStatus) -> Option<SubsonicView> {
    let status = match &plan.stage {
        Stage::Off => SubsonicStatus::Off,
        Stage::Blocked(causes) => SubsonicStatus::not_running(causes.clone()),
        Stage::Serving(serving) => match server {
            ServerStatus::Running { url } if *url == serving.server_url() => {
                SubsonicStatus::Running { addresses: serving.addresses() }
            }
            ServerStatus::Running { .. } | ServerStatus::Disabled => return None,
            ServerStatus::Error { error } => SubsonicStatus::not_running(vec![cause(error)]),
        },
    };
    Some(SubsonicView {
        status,
        networks: plan.networks.clone(),
        password_saved: plan.password_saved,
    })
}

fn cause(error: &ServerError) -> NotRunning {
    match error {
        ServerError::PortInUse { .. } => NotRunning::PortTaken,
        ServerError::InvalidConfig { detail }
        | ServerError::CredentialUnavailable { detail }
        | ServerError::BindFailed { detail }
        | ServerError::ServerFailed { detail } => NotRunning::Failed { detail: detail.clone() },
    }
}

/// The Subsonic view now, then each time it changes: a setting the server
/// follows, a network that comes or goes, and the server stopping on its own.
pub struct SubsonicWatch {
    plan: watch::Receiver<Plan>,
    server: ServerWatch,
    told: Option<SubsonicView>,
}

impl SubsonicWatch {
    pub(crate) fn new(plan: watch::Receiver<Plan>, server: ServerWatch) -> Self {
        Self { plan, server, told: None }
    }

    /// The next view, or none once the controller is gone.
    pub async fn next(&mut self) -> Option<SubsonicView> {
        loop {
            let plan = self.plan.borrow_and_update().clone();
            let server = self.server.current();
            if let Some(view) = view(&plan, &server) {
                if self.told.as_ref() != Some(&view) {
                    self.told = Some(view.clone());
                    return Some(view);
                }
            }
            tokio::select! {
                changed = self.plan.changed() => changed.ok()?,
                changed = self.server.changed() => changed.ok()?,
            }
        }
    }
}

impl bae_store::live_query::LiveSource<SubsonicView> for SubsonicWatch {
    fn next(&mut self) -> bae_store::live_query::NextValue<'_, SubsonicView> {
        Box::pin(async move { SubsonicWatch::next(self).await.map(Ok) })
    }
}
