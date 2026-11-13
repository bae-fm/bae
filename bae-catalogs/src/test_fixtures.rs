//! Values the catalog tests build the same way.

use crate::musicbrainz::{MbRecording, MbTrack};

/// `raw`, a whole HTTP response, with `Connection: close` added: a scripted
/// server that drops the connection after one answer says so, so a client
/// never sends its next request down a connection that is closing.
pub(crate) fn closing(raw: &str) -> String {
    raw.replacen("\r\n", "\r\nConnection: close\r\n", 1)
}

/// A MusicBrainz track numbered `number` whose recording is titled `title`,
/// with no credits and no length.
pub(crate) fn make_mb_track(number: &str, title: &str) -> MbTrack {
    MbTrack {
        position: None,
        number: Some(number.to_string()),
        title: None,
        length: None,
        recording: Some(MbRecording {
            id: None,
            title: Some(title.to_string()),
            artist_credit: vec![],
            relations: vec![],
        }),
        artist_credit: vec![],
    }
}
