// What a run asks the catalogs by catalog number, read off the requests the
// fake provider received.

/// The query pairs of every request whose target contains `path` and names
/// `key`, decoded.
fn requests_naming(
    fixture: &Fixture,
    path: &str,
    key: &str,
) -> Vec<std::collections::HashMap<String, String>> {
    fixture
        .provider
        .requests()
        .iter()
        .filter(|target| target.contains(path))
        .map(|target| {
            reqwest::Url::parse(&format!("http://fake{target}"))
                .expect("a request target parses")
                .query_pairs()
                .into_owned()
                .collect::<std::collections::HashMap<_, _>>()
        })
        .filter(|query| query.contains_key(key))
        .collect()
}

/// The folder states the number the disc ID's release carries, so the
/// number comes into effect: Discogs is searched for it under the artist
/// that release credits, as its words beside `catno`, and MusicBrainz, which
/// answers in one page, for the number alone.
#[tokio::test(flavor = "multi_thread")]
async fn a_rounds_catalog_number_search_names_the_artist_on_discogs_alone() {
    let fixture = Fixture::new("catalog-search-artist").await;
    fixture.use_discogs().await;
    let dir = fixture.disc_id_candidate("NJ-8255");
    let probed = fixture.probed_total_ms(&dir);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json_with_catalog("mb-catalog-1", "rg-catalog-1", &[probed, 0], "NJ 8255"),
    );
    fixture.provider.route(
        "/release/mb-catalog-1?",
        200,
        release_json_with_catalog("mb-catalog-1", "rg-catalog-1", &[probed, 0], "NJ 8255"),
    );
    fixture
        .provider
        .route("/database/search", 200, r#"{"results":[]}"#);
    fixture.scan(1).await;

    fixture.drain_automatic().await;
    fixture.await_identified_row(&dir).await;

    let musicbrainz: Vec<String> = requests_naming(&fixture, "/ws/2/release?", "query")
        .into_iter()
        .map(|query| query["query"].clone())
        .filter(|query| query.contains("catno:"))
        .collect();
    assert_eq!(
        musicbrainz,
        vec![r#"catno:"NJ 8255""#.to_string()],
        "MusicBrainz is asked for the number alone"
    );
    let discogs = requests_naming(&fixture, "/database/search", "catno");
    assert_eq!(discogs.len(), 1, "Discogs is asked once: {discogs:?}");
    assert_eq!(discogs[0]["catno"], "NJ 8255");
    assert_eq!(discogs[0].get("q").map(String::as_str), Some("Artist"));
    assert_eq!(
        discogs[0].get("label"),
        None,
        "a number in effect is searched under no label"
    );
}

/// The disc ID's release prints a number under its label and no barcode, and
/// the folder does not state the number: the release's pressing is looked up
/// on Discogs by that number, under its label and the artist it credits.
#[tokio::test(flavor = "multi_thread")]
async fn a_pressing_lookup_by_catalog_number_names_the_label_and_the_artist() {
    let fixture = Fixture::new("pressing-search-label").await;
    fixture.use_discogs().await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    let label_info = serde_json::json!([
        { "catalog-number": "LN-100", "label": { "name": "Label Name" } }
    ]);
    let mut disc: serde_json::Value = serde_json::from_str(&discid_json(
        "mb-pressing-1",
        "rg-pressing-1",
        &[probed, 0],
    ))
    .expect("the disc ID fixture parses");
    disc["releases"][0]["label-info"] = label_info.clone();
    fixture.provider.route("/discid/", 200, disc.to_string());
    let mut release: serde_json::Value = serde_json::from_str(&release_json(
        "mb-pressing-1",
        "rg-pressing-1",
        &[probed, 0],
    ))
    .expect("the release fixture parses");
    release["label-info"] = label_info;
    fixture
        .provider
        .route("/release/mb-pressing-1?", 200, release.to_string());
    fixture
        .provider
        .route("/database/search", 200, r#"{"results":[]}"#);
    fixture.scan(1).await;

    fixture.drain_automatic().await;
    fixture.await_identified_row(&dir).await;

    let discogs = requests_naming(&fixture, "/database/search", "catno");
    assert_eq!(discogs.len(), 1, "Discogs is asked once: {discogs:?}");
    assert_eq!(discogs[0]["catno"], "LN-100");
    assert_eq!(discogs[0].get("label").map(String::as_str), Some("Label Name"));
    assert_eq!(discogs[0].get("q").map(String::as_str), Some("Artist"));
}
