//! How many reads of one volume run at once, and which waits first.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::oneshot;

use crate::{AskedTurn, PictureTurn};

/// Which of a volume's places a read holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// One of the places a read of any turn takes.
    Any,
    /// The one place only a focused read takes.
    Focused,
}

/// A request that waits for a place.
struct Waiting {
    /// What the request asks now, read when a place is given.
    turn: AskedTurn,
    place: oneshot::Sender<Granted>,
}

struct GateState {
    free: usize,
    focused_free: bool,
    /// Oldest first.
    waiting: Vec<Waiting>,
}

impl GateState {
    /// Takes the next request to get a place out of the line, with the place:
    /// the one of the highest turn, the oldest among equals. None when nothing
    /// waits or no free place takes that request.
    fn next(&mut self) -> Option<(Waiting, Place)> {
        // A request that stopped waiting dropped its receiver.
        self.waiting.retain(|waiting| !waiting.place.is_closed());
        let (index, turn) = self
            .waiting
            .iter()
            .enumerate()
            .map(|(index, waiting)| (index, waiting.turn.now()))
            .max_by_key(|&(index, turn)| (turn, Reverse(index)))?;
        let place = if self.free > 0 {
            self.free -= 1;
            Place::Any
        } else if self.focused_free && turn == PictureTurn::Focused {
            self.focused_free = false;
            Place::Focused
        } else {
            return None;
        };
        Some((self.waiting.remove(index), place))
    }
}

/// The reads of one volume.
struct Gate {
    state: Mutex<GateState>,
}

impl Gate {
    fn new(reads_at_once: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(GateState {
                free: reads_at_once,
                focused_free: true,
                waiting: Vec::new(),
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, GateState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Returns once the request is granted one of the volume's places. `turn`
    /// may change while it waits.
    async fn wait_for_place(self: &Arc<Self>, mut turn: AskedTurn) -> Granted {
        let (sender, mut receiver) = oneshot::channel();
        self.lock().waiting.push(Waiting { turn: turn.clone(), place: sender });
        self.give_places();
        loop {
            tokio::select! {
                biased;
                given = &mut receiver => {
                    // The line holds the sender until it sends, and the gate
                    // that holds the line lives while this request does.
                    return given.expect("the gate sends a place to each request in its line");
                }
                () = turn.changed() => {
                    // A request that became focused may take the focused
                    // place now.
                    self.give_places();
                }
            }
        }
    }

    /// Gives free places to the requests that wait, in their turns.
    fn give_places(self: &Arc<Self>) {
        loop {
            let Some((waiting, place)) = self.lock().next() else { return };
            // A request that stopped waiting meanwhile hands the grant back,
            // and dropping it gives the place to the next in line. The lock is
            // free here, as the drop takes it.
            drop(waiting.place.send(Granted { gate: self.clone(), place }));
        }
    }
}

/// A place the gate granted a request, which goes back to the gate when this
/// drops, whether or not the request was still there to take it.
struct Granted {
    gate: Arc<Gate>,
    place: Place,
}

impl Drop for Granted {
    fn drop(&mut self) {
        {
            let mut state = self.gate.lock();
            match self.place {
                Place::Any => state.free += 1,
                Place::Focused => state.focused_free = true,
            }
        }
        self.gate.give_places();
    }
}

/// One of a volume's places for a read, held until it is dropped.
pub struct Permit {
    _granted: Granted,
}

/// The gates of the volumes originals are read from, one for each.
#[derive(Clone)]
pub struct Volumes {
    gates: Arc<Mutex<HashMap<Vec<u8>, Arc<Gate>>>>,
}

impl Volumes {
    pub fn new() -> Self {
        Self { gates: Arc::default() }
    }

    /// A place for one read of the volume named `volume`, waiting for one when
    /// none is free. The volume takes `reads_at_once` reads of any turn at
    /// once, and one more that only a focused read takes, so a focused read
    /// never waits for the reads of other turns that run. Among the requests
    /// that wait, the highest turn goes first, and the oldest among equals;
    /// `turn` says what this one asks, and may change while it waits. The
    /// first request for a volume sets how many it takes.
    pub async fn read_place(&self, volume: &[u8], reads_at_once: usize, turn: AskedTurn) -> Permit {
        let gate = self
            .gates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(volume.to_vec())
            .or_insert_with(|| Gate::new(reads_at_once))
            .clone();
        Permit { _granted: gate.wait_for_place(turn).await }
    }
}
