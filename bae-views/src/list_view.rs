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
//!
//! The view runs as a task of the owner that opens it ([`open`]), which
//! holds the list's reads. The screen holds a [`ListViewHandle`]: it sends
//! the rows on screen through it and receives the window's rows.

use crate::{ListRows, ListWindowValue, LiveListPosition, LiveListWindow};
use bae_store::list_windows::OrderKey;
use bae_store::live_query::LiveQueryError;
use bae_store::ListWindow;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::{mpsc, watch};
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

/// What wakes the view: an answer of one of its reads.
enum Answer {
    Window(Result<ListWindowValue, LiveQueryError>),
    Position(Result<crate::ListPosition, LiveQueryError>),
}

/// One list read for its screen: the window core keeps around the rows on
/// screen, with the position of its first row. The task [`ListView::open`]
/// returns runs it; the screen's [`ListViewHandle`] reports the rows on
/// screen and receives each value.
pub struct ListView {
    reads: ListReads,
    state: State,
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
    /// Whether `answer` and `start` changed since the view last sent them.
    fresh: bool,
}

impl State {
    /// The window's answer, placed, once both reads answer `request`.
    fn placed(&self) -> Option<Placed<'_>> {
        let answer = self.answer.as_ref()?;
        Some(Placed { window: &answer.window, start: self.start?, keys: &answer.keys })
    }

    /// The window's rows with the position of its first row, once both
    /// reads have answered since the view last sent them.
    fn take_fresh(&mut self) -> Option<ListViewValue> {
        if !self.fresh {
            return None;
        }
        let (Some(answer), Some(start)) = (&self.answer, self.start) else {
            return None;
        };
        self.fresh = false;
        Some(ListViewValue { start, rows: answer.rows.clone() })
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

    /// Keep `answer` when it answers the window last requested.
    fn take(&mut self, answer: Answer) -> Result<(), LiveQueryError> {
        match answer {
            Answer::Window(answer) => {
                let answer = answer?;
                if self.request.as_ref() == Some(&answer.window) {
                    self.answer = Some(answer);
                    self.fresh = true;
                }
            }
            Answer::Position(position) => {
                let position = position?;
                let asked = self.request.as_ref().and_then(|request| request.from.as_ref());
                if asked == Some(&position.key) {
                    self.start = Some(position.rows_before);
                    self.fresh = true;
                }
            }
        }
        Ok(())
    }

    /// The next answer of the window's read or the position's.
    async fn answer(&self) -> Answer {
        let window = async {
            match &self.window {
                Some(read) => read.next().await,
                None => std::future::pending().await,
            }
        };
        let position = async {
            match &self.position {
                Some(read) => read.next().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            answer = window => Answer::Window(answer),
            position = position => Answer::Position(position),
        }
    }

    /// End the view's reads.
    async fn cancel(&self) {
        if let Some(read) = &self.window {
            read.cancel().await;
        }
        if let Some(read) = &self.position {
            read.cancel().await;
        }
    }
}

impl ListView {
    /// Open the view of the list `reads` reads, with the rows `visible` on
    /// screen: the handle the screen holds, and the view's task, which its
    /// owner runs until it returns or `stop` is cancelled. The handle's
    /// `cancel` cancels `stop`.
    pub fn open(
        reads: ListReads,
        visible: Visible,
        stop: CancellationToken,
    ) -> (ListViewHandle, impl Future<Output = ()> + Send + 'static) {
        let (shown, watched) = watch::channel(visible);
        let (sent, received) = mpsc::channel(1);
        let view = ListView { reads, state: State::default() };
        let handle =
            ListViewHandle { visible: shown, values: tokio::sync::Mutex::new(received), stop };
        (handle, view.run(watched, sent))
    }

    /// Move the window where the rows `visible` on screen want it, once the
    /// window last asked for has answered and been placed, or at once when
    /// there is none yet.
    async fn place(&mut self, visible: Visible) -> Result<(), LiveQueryError> {
        if self.state.request.is_some() && self.state.placed().is_none() {
            return Ok(());
        }
        let window = match wanted(visible, self.state.placed()) {
            Move::Stay => return Ok(()),
            Move::To(window) => window,
            Move::Jump { position, limit } => {
                let from = (self.reads.key_at)(position).await?;
                ListWindow { from, limit }
            }
        };
        self.state.request(&self.reads, window)
    }

    /// Keep the window around the rows on screen `visible` reports, and send
    /// `values` the window's rows each time both reads have answered: the
    /// first once both answer, then one each time either answers again. A
    /// failed read is sent as its error, and the view goes on. A value the
    /// screen has not taken yet gives way to a later one. The view ends its
    /// reads and returns once the screen's handle is gone, or once its reads
    /// are cancelled, as closing the library cancels them: a cancelled read
    /// answers at once every time it is asked, so it has nothing more to
    /// send.
    async fn run(
        mut self,
        mut visible: watch::Receiver<Visible>,
        values: mpsc::Sender<Result<ListViewValue, LiveQueryError>>,
    ) {
        let mut unsent: Option<Result<ListViewValue, LiveQueryError>> = None;
        loop {
            let shown = *visible.borrow_and_update();
            match self.place(shown).await {
                Ok(()) => {}
                Err(LiveQueryError::Cancelled) => break,
                Err(error) => unsent = Some(Err(error)),
            }
            if let Some(value) = self.state.take_fresh() {
                unsent = Some(Ok(value));
            }
            tokio::select! {
                biased;
                changed = visible.changed() => {
                    if changed.is_err() {
                        break;
                    }
                }
                permit = values.reserve(), if unsent.is_some() => {
                    let Ok(permit) = permit else { break };
                    if let Some(value) = unsent.take() {
                        permit.send(value);
                    }
                }
                answer = self.state.answer() => {
                    match self.state.take(answer) {
                        Ok(()) => {}
                        Err(LiveQueryError::Cancelled) => break,
                        Err(error) => unsent = Some(Err(error)),
                    }
                }
            }
        }
        self.state.cancel().await;
    }
}

/// The screen's handle on one list's view: it reports the rows on screen,
/// and receives the window's rows. Cancelling it, or dropping it, ends the
/// view.
pub struct ListViewHandle {
    visible: watch::Sender<Visible>,
    values: tokio::sync::Mutex<mpsc::Receiver<Result<ListViewValue, LiveQueryError>>>,
    stop: CancellationToken,
}

impl ListViewHandle {
    /// The screen shows the rows `visible` now.
    pub fn set_visible(&self, visible: Visible) {
        self.visible.send_replace(visible);
    }

    /// The window's rows with the position of its first row: the first once
    /// both reads answer, then one each time either answers again. A failed
    /// read is answered as its error. Once the view has ended, or the handle
    /// is cancelled, the answer is that it was cancelled.
    pub async fn next(&self) -> Result<ListViewValue, LiveQueryError> {
        let mut values = self.values.lock().await;
        tokio::select! {
            biased;
            () = self.stop.cancelled() => Err(LiveQueryError::Cancelled),
            value = values.recv() => value.unwrap_or(Err(LiveQueryError::Cancelled)),
        }
    }

    /// End the view, and settle a waiting `next`.
    pub fn cancel(&self) {
        self.stop.cancel();
    }
}

impl Drop for ListViewHandle {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

#[cfg(test)]
#[path = "list_view_tests.rs"]
mod tests;
