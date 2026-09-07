//! Own a query until its value receiver closes. Runtime progress cannot
//! cancel and requeue a pending database read or result processor.

use crate::import::{CandidateRuntimeChange, CandidateRuntimeSnapshot, TriageRuntimeFacts};
use crate::library::LibraryError;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use tokio::sync::{broadcast, mpsc};

pub(super) fn subscribe<P, V>(
    runtime: &tokio::runtime::Handle,
    keys: BTreeSet<String>,
    mut query: coven::LiveQuery<P>,
    initial: HashMap<String, CandidateRuntimeSnapshot>,
    mut changes: broadcast::Receiver<CandidateRuntimeChange>,
    reread: impl Fn() -> HashMap<String, CandidateRuntimeSnapshot> + Send + 'static,
    resolve: impl Fn(&P, &BTreeMap<String, TriageRuntimeFacts>) -> V + Send + 'static,
) -> mpsc::UnboundedReceiver<Result<V, LibraryError>>
where
    P: Clone + PartialEq + Send + 'static,
    V: Clone + PartialEq + Send + 'static,
{
    let (tx, rx) = mpsc::unbounded_channel();
    runtime.spawn(async move {
        let selected_facts = |runtimes: &HashMap<String, CandidateRuntimeSnapshot>| {
            crate::import::list::facts_of(runtimes).into_iter().filter(|(key, _)| keys.contains(key)).collect::<BTreeMap<_, _>>()
        };
        let mut facts = selected_facts(&initial);
        let mut projection = None;
        let mut delivered = None;
        loop {
            let next = query.next();
            tokio::pin!(next);
            loop {
                tokio::select! {
                    biased;
                    () = tx.closed() => return,
                    value = &mut next => {
                        match value {
                            Ok(value) => {
                                let resolved = resolve(&value, &facts);
                                projection = Some(value);
                                if delivered.as_ref() != Some(&resolved) {
                                    delivered = Some(resolved.clone());
                                    if tx.send(Ok(resolved)).is_err() { return; }
                                }
                            }
                            Err(error) => {
                                let error = match error {
                                    coven::CovenError::Database(error) => *error,
                                    other => coven::DbError::Message(other.to_string()),
                                };
                                delivered = None;
                                if tx.send(Err(LibraryError::Database(error))).is_err() { return; }
                            }
                        }
                        break;
                    }
                    change = changes.recv() => {
                        let before = facts.clone();
                        match change {
                            Ok(CandidateRuntimeChange::Updated { key, runtime }) => {
                                if !keys.contains(&key) { continue; }
                                let next = TriageRuntimeFacts::of(&runtime);
                                if next == TriageRuntimeFacts::default() { facts.remove(&key); }
                                else { facts.insert(key, next); }
                            }
                            Ok(CandidateRuntimeChange::Removed { key }) => { facts.remove(&key); }
                            Ok(CandidateRuntimeChange::Reset { runtimes }) => { facts = selected_facts(&runtimes); }
                            Err(broadcast::error::RecvError::Lagged(count)) => {
                                tracing::warn!("selected import subscription dropped {count} runtime changes; rereading selected runtimes");
                                facts = selected_facts(&reread());
                            }
                            Err(broadcast::error::RecvError::Closed) => return,
                        }
                        if facts == before { continue; }
                        if let Some(projection) = &projection {
                            let resolved = resolve(projection, &facts);
                            if delivered.as_ref() != Some(&resolved) {
                                delivered = Some(resolved.clone());
                                if tx.send(Ok(resolved)).is_err() { return; }
                            }
                        }
                    }
                }
            }
        }
    });
    rx
}

#[cfg(test)]
#[path = "value_subscription_tests.rs"]
mod tests;
