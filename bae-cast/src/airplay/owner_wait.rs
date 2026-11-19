//! Waiting for the owner of an AirPlay receiver to accept bae.
//!
//! A Mac set to allow AirPlay for anyone on its network asks its owner to
//! accept each new sender, and asks again once an approval lapses. It holds
//! the sender's first request on a connection until the owner decides, so the
//! reply to that request takes as long as the owner does.

use std::io;
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

use super::rtsp::CONTROL_TIMEOUT;

/// How long a receiver may hold a reply for its owner before the cast fails
/// as one for a receiver that does not answer.
pub(crate) const OWNER_DECIDES_WITHIN: Duration = Duration::from_secs(60);

/// How long a receiver holds a reply before bae tells the person that the
/// receiver may be asking its owner.
pub(crate) const NOTICE_AFTER: Duration = Duration::from_secs(5);

/// How the wait for a reply is bounded. Tests replace it with one that
/// decides when each wait ends, so no test waits on a clock.
pub(crate) trait ReplyTimer: Send + Sync {
    /// Block until `stream` has bytes to read or has closed, and return true,
    /// or until `limit` passes, and return false.
    fn readable_within(&self, stream: &TcpStream, limit: Duration) -> io::Result<bool>;
}

/// The wait on the system's clock, through the socket's read timeout.
pub(crate) struct SocketTimer;

impl ReplyTimer for SocketTimer {
    fn readable_within(&self, stream: &TcpStream, limit: Duration) -> io::Result<bool> {
        stream.set_read_timeout(Some(limit))?;
        let peeked = stream.peek(&mut [0u8; 1]);
        stream.set_read_timeout(Some(CONTROL_TIMEOUT))?;
        match peeked {
            Ok(_) => Ok(true),
            Err(error)
                if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }
}

/// How one connection waits for its first reply: up to
/// [`OWNER_DECIDES_WITHIN`], telling `deciding` when the wait passes
/// [`NOTICE_AFTER`] and when the reply then comes.
pub(crate) struct OwnerWait {
    timer: Arc<dyn ReplyTimer>,
    deciding: Box<dyn Fn(bool) + Send>,
}

impl OwnerWait {
    pub(crate) fn new(
        timer: Arc<dyn ReplyTimer>,
        deciding: impl Fn(bool) + Send + 'static,
    ) -> Self {
        OwnerWait { timer, deciding: Box::new(deciding) }
    }

    /// Wait until `stream` has its reply. A wait that reaches
    /// [`OWNER_DECIDES_WITHIN`] fails as a timeout.
    pub(super) fn for_reply(&self, stream: &TcpStream) -> io::Result<()> {
        if self.timer.readable_within(stream, NOTICE_AFTER)? {
            return Ok(());
        }
        (self.deciding)(true);
        if !self.timer.readable_within(stream, OWNER_DECIDES_WITHIN - NOTICE_AFTER)? {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the receiver held its reply for a minute",
            ));
        }
        (self.deciding)(false);
        Ok(())
    }
}

#[cfg(test)]
#[path = "owner_wait_tests.rs"]
mod tests;
