//! System endpoints: liveness, licensing, and the music-folder list.

use std::collections::HashMap;

use axum::extract::Query;
use axum::response::Response;

use crate::endpoints::respond;
use crate::envelope::Element;
use crate::params::Params;

/// `ping` — an empty ok envelope. Clients use it to probe connectivity and
/// credentials.
pub(crate) async fn ping(Query(params): Query<HashMap<String, String>>) -> Response {
    let params = Params(params);
    respond(&params.format(), Ok(None))
}

/// `getLicense` — bae has no licensing; report a valid, non-expiring license so
/// clients that gate on it proceed.
pub(crate) async fn get_license(Query(params): Query<HashMap<String, String>>) -> Response {
    let params = Params(params);
    let license = Element::new("license").attr("valid", true);
    respond(&params.format(), Ok(Some(license)))
}

/// `getMusicFolders` — bae has no folder tree, so it presents one synthetic
/// folder. Clients that require the call to enumerate a library get a single
/// folder covering everything.
pub(crate) async fn get_music_folders(Query(params): Query<HashMap<String, String>>) -> Response {
    let params = Params(params);
    let folders = Element::new("musicFolders").child(
        Element::new("musicFolder")
            .attr("id", 0_i64)
            .attr("name", "bae"),
    );
    respond(&params.format(), Ok(Some(folders)))
}
