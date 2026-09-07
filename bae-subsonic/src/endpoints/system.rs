//! System endpoints: liveness, licensing, and the music-folder list.

use crate::envelope::Element;
use crate::error::SubError;
use crate::params::Params;
use crate::AppState;

/// `ping` — an empty ok envelope. Clients use it to probe connectivity and
/// credentials.
pub(crate) async fn ping(_: AppState, _: Params) -> Result<Option<Element>, SubError> {
    Ok(None)
}

/// `getLicense` — bae has no licensing; report a valid, non-expiring license so
/// clients that gate on it proceed.
pub(crate) async fn get_license(_: AppState, _: Params) -> Result<Option<Element>, SubError> {
    Ok(Some(Element::new("license").attr("valid", true)))
}

/// `getMusicFolders` — bae has no folder tree, so it presents one synthetic
/// folder. Clients that require the call to enumerate a library get a single
/// folder covering everything.
pub(crate) async fn get_music_folders(_: AppState, _: Params) -> Result<Option<Element>, SubError> {
    Ok(Some(
        Element::new("musicFolders").child(
            Element::new("musicFolder")
                .attr("id", 0_i64)
                .attr("name", "bae"),
        ),
    ))
}
