async fn serve_one(mut stream: tokio::net::TcpStream, state: Arc<Mutex<FakeState>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    // One request per connection: every response says `Connection: close`, so
    // headers always arrive in the first read or two and there is no pipelining
    // to unpick.
    while !buffer.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    }
    let head = String::from_utf8_lossy(&buffer).to_string();
    let target = head
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();

    let (status, body, gate) = {
        let mut state = state.lock().unwrap();
        state.requests.push(target.clone());
        let (status, body) = state
            .routes
            .iter()
            .find(|(needle, _, _)| target.contains(needle.as_str()))
            .map(|(_, status, body)| (*status, body.clone()))
            .unwrap_or((404, "{}".to_string()));
        let gate = state
            .gate
            .as_ref()
            .filter(|(needle, _)| target.contains(needle.as_str()))
            .map(|(_, gate)| gate.clone());
        (status, body, gate)
    };
    if let Some(gate) = gate {
        // The hold ends by closing the semaphore, so this wait only ever ends
        // — it never takes a permit, and no release can be missed.
        let _ = gate.acquire().await;
    }

    let response = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

// ── Canned MusicBrainz payloads ─────────────────────────────────────────────

/// A release as the disc-ID and release-lookup endpoints return it: one medium
/// whose tracks carry `length`, which is what makes the disc-ID path free.
fn release_json(release_id: &str, group_id: &str, track_lengths: &[u64]) -> String {
    let tracks: Vec<String> = track_lengths
        .iter()
        .enumerate()
        .map(|(i, length)| {
            format!(
                r#"{{"position":{},"number":"{}","title":"Track {}","length":{length}}}"#,
                i + 1,
                i + 1,
                i + 1
            )
        })
        .collect();
    format!(
        r#"{{"id":"{release_id}","title":"Album","artist-credit":[{{"name":"Artist"}}],
            "release-group":{{"id":"{group_id}"}},
            "media":[{{"tracks":[{}]}}],"relations":[],
            "cover-art-archive":{{"front":false,"darkened":false}}}}"#,
        tracks.join(",")
    )
}

/// A release under a title and artist of its own — for a test that has to tell
/// two releases apart by what a row shows.
fn titled_release_json(release_id: &str, group_id: &str, title: &str, artist: &str) -> String {
    format!(
        r#"{{"id":"{release_id}","title":"{title}",
            "artist-credit":[{{"name":"{artist}"}}],
            "release-group":{{"id":"{group_id}"}},
            "media":[{{"tracks":[
                {{"position":1,"number":"1","title":"Track Title 1","length":180000}},
                {{"position":2,"number":"2","title":"Track Title 2","length":180000}}
            ]}}],"relations":[],
            "cover-art-archive":{{"front":true,"darkened":false}}}}"#
    )
}

fn discid_json(release_id: &str, group_id: &str, track_lengths: &[u64]) -> String {
    let mut release: serde_json::Value =
        serde_json::from_str(&release_json(release_id, group_id, track_lengths))
            .expect("release fixture parses");
    release["media"][0]["discs"] = serde_json::json!([{ "id": FIXTURE_DISC_ID }]);
    serde_json::json!({ "releases": [release] }).to_string()
}

/// The same disc-ID answer for a release that prints a barcode. MusicBrainz
/// states a release's barcode on every answer that names it, so a disc-ID
/// result carries the code that pairs it with the Discogs record of the same
/// pressing.
fn discid_json_stating_barcode(
    release_id: &str,
    group_id: &str,
    track_lengths: &[u64],
    barcode: &str,
) -> String {
    let mut answer: serde_json::Value =
        serde_json::from_str(&discid_json(release_id, group_id, track_lengths))
            .expect("the disc ID fixture parses");
    answer["releases"][0]["barcode"] = serde_json::json!(barcode);
    answer.to_string()
}

/// A search hit as `ws/2/release?query=…` returns it: no `media`, hence no
/// lengths and no count, so the Ready rule has nothing to check until the lead
/// is settled.
fn search_json(release_id: &str, group_id: &str) -> String {
    format!(
        r#"{{"releases":[{{"id":"{release_id}","title":"Album",
            "artist-credit":[{{"name":"Artist"}}],
            "release-group":{{"id":"{group_id}"}},"label-info":[]}}]}}"#
    )
}

/// The same, for hits that state the barcode they were found by — what pairs
/// a MusicBrainz release with the Discogs record of the same pressing, and
/// what tells two different pressings apart.
fn barcode_search_json(releases: &[(&str, &str, &str)]) -> String {
    let releases: Vec<String> = releases
        .iter()
        .map(|(release_id, group_id, barcode)| {
            format!(
                r#"{{"id":"{release_id}","title":"Album",
                    "artist-credit":[{{"name":"Artist"}}],
                    "release-group":{{"id":"{group_id}"}},
                    "barcode":"{barcode}","label-info":[]}}"#
            )
        })
        .collect();
    format!(r#"{{"releases":[{}]}}"#, releases.join(","))
}

/// A Discogs search hit as `database/search?barcode=…` returns it. The title
/// is the "Artist - Album" form Discogs answers with, which is what decides
/// whether this lands on the same album card as the MusicBrainz result.
fn discogs_search_json(release_id: &str, barcode: &str) -> String {
    format!(
        r#"{{"results":[{{"id":{release_id},"type":"release",
            "title":"Artist - Album","year":"1996","barcode":["{barcode}"]}}]}}"#
    )
}

/// A one-track Discogs release with no master, as `releases/{id}` returns it.
fn discogs_release_json(release_id: &str) -> String {
    serde_json::json!({
        "id": release_id.parse::<u64>().expect("a numeric test Discogs release id"),
        "title": "Album",
        "year": 1996,
        "formats": [{ "name": "CD" }],
        "artists": [{ "id": 1, "name": "Artist" }],
        "tracklist": [{
            "position": "1",
            "title": "Track 1",
            "duration": "0:01",
            "type_": "track",
            "artists": [],
        }],
    })
    .to_string()
}
