//! One long list as its screen reads it: one live window of the list, which
//! core places and sizes from the rows the screen shows, and the window's
//! position, a live count of the rows before its first row.
//!
//! The screen reports the first and last row it shows, and core keeps the
//! window around them: the rows on screen plus about one screen above and
//! one below. When the rows on screen come near the window's edge, core
//! starts the window again a screen above them, from a row the window holds,
//! whose sort values are the new window's start; a row the window doesn't
//! hold, after a jump, core finds by offset once. The view holds the latest
//! answer of the window's live query and of its position's, until each
//! reruns, and nothing else.

use crate::{ListRows, ListWindowValue, LiveListPosition, LiveListWindow};
use bae_store::list_windows::OrderKey;
use bae_store::live_query::LiveQueryError;
use bae_store::ListWindow;
use std::future::Future;
use std::pin::Pin;
use tokio_util::sync::CancellationToken;

/// The rows a screen shows: the positions of its first and last row on
/// screen, `first` at most `last`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Visible {
    first: u64,
    last: u64,
}

impl Visible {
    /// The rows from `first` to `last`, either way round.
    pub fn new(first: u64, last: u64) -> Self {
        Self { first: first.min(last), last: first.max(last) }
    }

    /// How many rows the screen shows.
    fn rows(self) -> u64 {
        self.last - self.first + 1
    }
}

/// The window's rows as the screen draws them: the first at position
/// `start`, each next one at the position after.
#[derive(Debug, Clone)]
pub struct ListViewValue {
    pub start: u64,
    pub rows: ListRows,
}

/// The key of the row at a position, read once by offset: `None` for a list
/// with no rows. The read fails as the list's reads do, and is cancelled
/// once the list closes.
pub type KeyAt = Pin<Box<dyn Future<Output = Result<Option<OrderKey>, LiveQueryError>> + Send>>;

/// The reads of one list that its view opens: its window, the position of a
/// key, and the key at a position.
pub struct ListReads {
    pub window: Box<dyn Fn(ListWindow) -> LiveListWindow + Send + Sync>,
    pub position: Box<dyn Fn(OrderKey) -> LiveListPosition + Send + Sync>,
    pub key_at: Box<dyn Fn(u64) -> KeyAt + Send + Sync>,
}

/// Where the view moves its window, from the rows on screen and the window's
/// last answer.
#[derive(Debug, Clone, PartialEq)]
enum Move {
    /// The window holds the rows on screen and a margin on each side.
    Stay,
    /// A window whose start is the list's top, or a row the window holds.
    To(ListWindow),
    /// A window starting at a position no row the window holds is at.
    Jump { position: u64, limit: u64 },
}

/// The window's last answer as the screen places it: the request it
/// answers, the position of its first row, and its rows' keys.
struct Placed<'a> {
    window: &'a ListWindow,
    start: u64,
    keys: &'a [OrderKey],
}

/// Where the window goes for the rows `visible` on screen, given where it
/// stands. The window holds the rows on screen and one screen of rows on
/// each side, and moves once the rows on screen come within half a screen
/// of an edge past which the list goes on.
fn wanted(visible: Visible, placed: Option<Placed<'_>>) -> Move {
    let screen = visible.rows();
    let start = visible.first.saturating_sub(screen);
    let limit = 3 * screen;
    let Some(placed) = placed else {
        return start_at(start, limit, None);
    };
    let held = placed.keys.len() as u64;
    let end = placed.start + held;
    let near = screen.div_ceil(2);
    let short = placed.window.limit < limit;
    let near_top = placed.start > 0 && visible.first < placed.start + near;
    let near_end = held == placed.window.limit && visible.last + near >= end;
    if !(short || near_top || near_end) {
        return Move::Stay;
    }
    start_at(start, limit, Some(&placed))
}

/// The window of `limit` rows from the row at `position`: the list's top, a
/// row `placed` holds, or else a jump.
fn start_at(position: u64, limit: u64, placed: Option<&Placed<'_>>) -> Move {
    if position == 0 {
        return Move::To(ListWindow::top(limit));
    }
    let held = placed.and_then(|placed| {
        let at = usize::try_from(position.checked_sub(placed.start)?).ok()?;
        placed.keys.get(at).cloned()
    });
    match held {
        Some(from) => Move::To(ListWindow { from: Some(from), limit }),
        None => Move::Jump { position, limit },
    }
}

/// What wakes the view: an answer of one of its reads, or other rows on
/// screen.
enum Answer {
    Window(Result<ListWindowValue, LiveQueryError>),
    Position(Result<crate::ListPosition, LiveQueryError>),
    Moved,
}

/// One list read for its screen: the window core keeps around the rows on
/// screen, with the position of its first row.
pub struct ListView {
    reads: ListReads,
    /// The rows on screen, as the screen last reported them.
    visible: std::sync::Mutex<Visible>,
    /// Wakes a waiting `next` when the screen reports other rows.
    moved: tokio::sync::Notify,
    cancellation: CancellationToken,
    state: tokio::sync::Mutex<State>,
}

/// The view's reads and the latest answer of each, for the window last
/// requested.
#[derive(Default)]
struct State {
    window: Option<LiveListWindow>,
    position: Option<LiveListPosition>,
    /// The window last requested.
    request: Option<ListWindow>,
    /// The window's answer to `request`.
    answer: Option<ListWindowValue>,
    /// The position of `request`'s first row: 0 for the list's top, else
    /// the position's answer for its key.
    start: Option<u64>,
    /// Whether `answer` and `start` changed since `next` last returned them.
    fresh: bool,
}

impl State {
    /// The window's answer, placed, once both reads answer `request`.
    fn placed(&self) -> Option<Placed<'_>> {
        let answer = self.answer.as_ref()?;
        Some(Placed { window: &answer.window, start: self.start?, keys: &answer.keys })
    }

    /// Ask for `window`: the window's read moves to it, and the position's
    /// read to its key. Neither answer for the window before holds.
    fn request(&mut self, reads: &ListReads, window: ListWindow) -> Result<(), LiveQueryError> {
        match &window.from {
            None => {
                self.position = None;
                self.start = Some(0);
            }
            Some(key) => {
                match &self.position {
                    Some(position) => {
                        position.set(key.clone())?;
                    }
                    None => self.position = Some((reads.position)(key.clone())),
                }
                self.start = None;
            }
        }
        match &self.window {
            Some(read) => {
                read.set(window.clone())?;
            }
            None => self.window = Some((reads.window)(window.clone())),
        }
        self.answer = None;
        self.fresh = false;
        self.request = Some(window);
        Ok(())
    }
}

impl ListView {
    /// The view of the list `reads` reads, with the rows `visible` on
    /// screen. It opens its reads on the first `next`.
    pub fn new(reads: ListReads, visible: Visible) -> Self {
        Self {
            reads,
            visible: std::sync::Mutex::new(visible),
            moved: tokio::sync::Notify::new(),
            cancellation: CancellationToken::new(),
            state: tokio::sync::Mutex::new(State::default()),
        }
    }

    /// The screen shows the rows `visible` now.
    pub fn set_visible(&self, visible: Visible) {
        *self.visible.lock().expect("list view's rows on screen mutex poisoned") = visible;
        self.moved.notify_one();
    }

    /// The window's rows with the position of its first row: the first once
    /// both reads answer, then one each time either answers again. A failed
    /// read is answered as its error, and the view goes on.
    pub async fn next(&self) -> Result<ListViewValue, LiveQueryError> {
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => Err(LiveQueryError::Cancelled),
            value = self.place_and_read() => value,
        }
    }

    async fn place_and_read(&self) -> Result<ListViewValue, LiveQueryError> {
        let mut state = self.state.lock().await;
        loop {
            self.place(&mut state).await?;
            if state.fresh {
                if let (Some(answer), Some(start)) = (&state.answer, state.start) {
                    let value = ListViewValue { start, rows: answer.rows.clone() };
                    state.fresh = false;
                    return Ok(value);
                }
            }
            match self.answer_or_move(&state).await {
                Answer::Window(answer) => {
                    let answer = answer?;
                    if state.request.as_ref() == Some(&answer.window) {
                        state.answer = Some(answer);
                        state.fresh = true;
                    }
                }
                Answer::Position(position) => {
                    let position = position?;
                    let asked = state.request.as_ref().and_then(|request| request.from.as_ref());
                    if asked == Some(&position.key) {
                        state.start = Some(position.rows_before);
                        state.fresh = true;
                    }
                }
                Answer::Moved => {}
            }
        }
    }

    /// The next answer of the window's read or the position's, or the
    /// screen reporting other rows, whichever comes first.
    async fn answer_or_move(&self, state: &State) -> Answer {
        let window = async {
            match &state.window {
                Some(read) => read.next().await,
                None => std::future::pending().await,
            }
        };
        let position = async {
            match &state.position {
                Some(read) => read.next().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            answer = window => Answer::Window(answer),
            position = position => Answer::Position(position),
            () = self.moved.notified() => Answer::Moved,
        }
    }

    /// Move the window where the rows on screen want it, once the window
    /// last asked for has answered and been placed, or at once when there is
    /// none yet.
    async fn place(&self, state: &mut State) -> Result<(), LiveQueryError> {
        let visible = *self.visible.lock().expect("list view's rows on screen mutex poisoned");
        if state.request.is_some() && state.placed().is_none() {
            return Ok(());
        }
        let window = match wanted(visible, state.placed()) {
            Move::Stay => return Ok(()),
            Move::To(window) => window,
            Move::Jump { position, limit } => {
                let from = (self.reads.key_at)(position).await?;
                ListWindow { from, limit }
            }
        };
        state.request(&self.reads, window)
    }

    /// End the view's reads and settle a waiting `next`.
    pub async fn cancel(&self) {
        self.cancellation.cancel();
        let state = self.state.lock().await;
        if let Some(read) = &state.window {
            read.cancel().await;
        }
        if let Some(read) = &state.position {
            read.cancel().await;
        }
    }
}

#[cfg(test)]
#[path = "list_view_tests.rs"]
mod tests;
