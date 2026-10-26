//! The placement table and the list's own placement agree: over a queue in
//! every state the rules tell apart, each row is on the same tab, in the same
//! state, under the same group header.

use super::super::super::import_list::load_import_queue_on;
use super::super::{ALBUM_A, ARTIST_A, RELEASE_A};
use super::*;
use crate::import::list::{flatten, ImportListRequest, ImportListView, ItemRef};
use crate::import::{NeedsYouReason, PendingState, TriageTab};
use std::collections::BTreeSet;

/// Where one row sits: its tab, its state and why it needs the person, and
/// the group listing it.
type Seat = (String, Option<String>, Option<String>, Option<String>);

#[tokio::test]
async fn the_placement_agrees_with_the_lists_own() {
    let lib = Library::new().await;
    let key = |display_path: &str| lib.path(display_path);
    let mut sql = String::new();
    for (display_path, kind) in [
        ("Plain", "valid"),
        ("Artist/Early", "valid"),
        ("Artist/Late", "valid"),
        ("Artist", "valid"),
        ("Box/Broken", "invalid"),
        ("Box", "valid"),
        ("Pending Scan", "tentative"),
        ("Skipped", "valid"),
        ("Imported", "valid"),
        ("Failed", "valid"),
        ("Linked", "valid"),
        ("Several", "valid"),
        ("Fits", "valid"),
        ("Disagrees", "valid"),
        ("Unread", "valid"),
        ("Kept", "valid"),
        ("No Tracklist", "valid"),
        ("Medium", "valid"),
        ("Not Found", "valid"),
        ("Manual", "valid"),
        ("Lookup Failed", "valid"),
        ("Broke", "valid"),
        ("Importing", "valid"),
        ("Identifying", "valid"),
        ("Reading Pick", "valid"),
        ("Not Written", "valid"),
        ("Older", "valid"),
        ("Disc 1", "valid"),
        ("Disc 2", "valid"),
    ] {
        sql.push_str(&lib.scan_as(display_path, kind));
    }
    for (display_path, date) in [("Artist/Early", 100), ("Artist/Late", 300), ("Plain", 200)] {
        sql.push_str(&lib.discovered(display_path, date));
    }
    for display_path in [
        "Plain",
        "Imported",
        "Failed",
        "Linked",
        "Several",
        "Fits",
        "Disagrees",
        "Unread",
        "Kept",
        "No Tracklist",
        "Medium",
        "Not Found",
        "Manual",
        "Lookup Failed",
        "Broke",
        "Not Written",
        "Older",
    ] {
        sql.push_str(&lib.state(display_path));
    }
    sql.push_str(&lib.draft("Plain", "Plain Title"));
    for (display_path, kind, track_count) in [
        ("Several", "found", Some(11)),
        ("Fits", "found", Some(11)),
        ("Disagrees", "found", Some(11)),
        ("Unread", "found", Some(11)),
        ("Kept", "found", Some(11)),
        ("No Tracklist", "found", Some(11)),
        ("Medium", "found", Some(11)),
        ("Not Found", "not_found", None),
        ("Manual", "manual_only", Some(11)),
        ("Lookup Failed", "failed", Some(11)),
        ("Broke", "error", None),
        ("Not Written", "not_found", None),
        ("Older", "not_found", None),
    ] {
        sql.push_str(&lib.verdict(display_path, kind, track_count));
    }
    let fits = (Some("listed"), Some(11));
    sql.push_str(&lib.found_release("Several", 0, 0, fits));
    sql.push_str(&lib.found_release("Several", 1, 1, fits));
    sql.push_str(&lib.found_release("Fits", 0, 0, fits));
    sql.push_str(&lib.found_release("Disagrees", 0, 0, (Some("listed"), Some(9))));
    sql.push_str(&lib.found_release("Unread", 0, 0, fits));
    sql.push_str(&lib.found_release("Kept", 0, 0, fits));
    sql.push_str(&lib.found_release("No Tracklist", 0, 0, (Some("nothing"), None)));
    sql.push_str(&lib.found_release("Medium", 0, 0, fits));
    sql.push_str(&format!(
        "UPDATE import_candidate_match SET document_failure = 'timeout'
         WHERE content_hash = '{unread}';
         UPDATE import_candidate_verdict SET kept_own_draft = 1 WHERE content_hash = '{kept}';
         UPDATE import_candidate_verdict SET medium_conflict = 'not_cd_audio'
         WHERE content_hash = '{medium}';
         UPDATE import_candidate_state SET edit_revision = 5 WHERE content_hash = '{older}';
         INSERT INTO skipped_import_candidates (candidate_path) VALUES ('{skipped}');
         INSERT INTO import_candidate_failure (content_hash, error, failed_at)
         VALUES ('{failed}', 'it broke', '2026-01-01T00:00:00Z');
         INSERT INTO import_candidate_release_link (content_hash, kind) VALUES ('{linked}', 'album');
         INSERT INTO import_candidate_release_link_album (content_hash, kind, catalog, album_id)
         VALUES ('{linked}', 'album', 'musicbrainz', 'group-1');
         INSERT INTO albums (id, title, is_compilation, created_at)
         VALUES ('{ALBUM_A}', 'Album Title', 0, '2026-01-01T00:00:00Z');
         INSERT INTO releases (id, album_id, created_at, album_order)
         VALUES ('{RELEASE_A}', '{ALBUM_A}', '2026-01-03T00:00:00Z', 0);
         INSERT INTO local_release_origin (release_id, content_hash)
         VALUES ('{RELEASE_A}', '{imported}');
         INSERT INTO artists (id, name, name_key, created_at)
         VALUES ('{ARTIST_A}', 'Picked Artist', 'picked artist', '2026-01-01T00:00:00Z');
         INSERT INTO import_candidate_album_artist_assignment
             (content_hash, position, assignment_kind, artist_id)
         VALUES ('{plain}', 0, 'picked', '{ARTIST_A}');
         INSERT INTO import_candidate_standing (candidate_key, import, import_id)
         VALUES ('{importing}', 'running', 'import-1');
         INSERT INTO import_candidate_standing
             (candidate_key, identification, content_hash, file_edit_revision, identify_run)
         VALUES ('{identifying}', 'running', '{identifying_hash}', 0, 1);
         INSERT INTO import_candidate_standing
             (candidate_key, identification, content_hash, file_edit_revision, identify_run,
              import_asked)
         VALUES ('{reading_pick}', 'reading_pick', '{reading_pick_hash}', 0, 1, 0);
         INSERT INTO import_candidate_standing
             (candidate_key, identification, content_hash, file_edit_revision,
              save_failure_error)
         VALUES ('{not_written}', 'not_written', '{not_written_hash}', 0, 'disk full');
         INSERT INTO release_grouping (key, anchor_folder, combined, author, blocked,
                                       blocked_subject)
         VALUES ('{discs}', NULL, 1, 'user', 'source_gone', 'Disc 2');
         INSERT INTO release_grouping_member (grouping_key, position, member_key, member_folder)
         VALUES ('{discs}', 0, '{disc_1}', '{disc_1}');",
        unread = hash("Unread"),
        kept = hash("Kept"),
        medium = hash("Medium"),
        older = hash("Older"),
        skipped = key("Skipped"),
        failed = hash("Failed"),
        linked = hash("Linked"),
        imported = hash("Imported"),
        plain = hash("Plain"),
        importing = key("Importing"),
        identifying = key("Identifying"),
        identifying_hash = hash("Identifying"),
        reading_pick = key("Reading Pick"),
        reading_pick_hash = hash("Reading Pick"),
        not_written = key("Not Written"),
        not_written_hash = hash("Not Written"),
        discs = key("Discs"),
        disc_1 = key("Disc 1"),
    ));
    sql.push_str(&format!(
        "INSERT INTO scan_candidate
             (watched_folder_path, path, generation, kind, name, display_path, folder,
              file_root, scope, content_hash, grouping_key, source_kind)
         VALUES ('{root}', '{discs}', 1, 'valid', 'Discs', 'Discs', '{disc_1}', '{disc_1}',
                 'recursive', '{hash}', '{discs}', 'grouping');",
        root = lib.root,
        discs = key("Discs"),
        disc_1 = key("Disc 1"),
        hash = hash("Discs"),
    ));
    lib.run(&sql).await;

    let rows = lib.db.read(|sql| load_import_queue_on(&sql)).await.unwrap();
    let mut listed: BTreeMap<String, Seat> = BTreeMap::new();
    let mut headers: BTreeSet<(String, String)> = BTreeSet::new();
    for tab in [TriageTab::Pending, TriageTab::Done, TriageTab::Skipped] {
        let flat = flatten(
            &rows,
            &ImportListRequest {
                view: ImportListView {
                    tab,
                    ..ImportListView::default()
                },
                ..ImportListRequest::default()
            },
        )
        .unwrap();
        let mut group = None;
        for item in &flat.items {
            match item {
                ItemRef::Header(index) => {
                    let key = &flat.headers[*index].group.key;
                    headers.insert((
                        key.watched_folder_path.clone(),
                        key.relative_folder_path.clone(),
                    ));
                    group = Some(key.relative_folder_path.clone());
                }
                ItemRef::Candidate {
                    index,
                    is_group_member,
                } => {
                    let placed = &flat.rows[*index];
                    let (state, reason) = match &placed.live.standing {
                        Some(standing) => (Some(standing.state()), reason(standing)),
                        None => (None, None),
                    };
                    listed.insert(
                        placed.row.candidate_key.clone(),
                        (
                            tab_name(tab).to_string(),
                            state.map(state_name).map(str::to_string),
                            reason.map(reason_name).map(str::to_string),
                            is_group_member.then(|| group.clone()).flatten(),
                        ),
                    );
                }
                ItemRef::Invalid { index, .. } => {
                    listed.insert(
                        rows.candidates[*index].path.clone(),
                        (tab_name(tab).to_string(), None, None, None),
                    );
                }
            }
        }
    }

    let placed = lib.placed().await;
    let seats: BTreeMap<String, Seat> = placed
        .iter()
        .filter(|((_, item, _), _)| *item != "group")
        .map(|((_, _, key), row)| {
            (
                key.clone(),
                (
                    row.tab.clone(),
                    row.pending_state.clone(),
                    row.needs_you_reason.clone(),
                    row.group_name.clone(),
                ),
            )
        })
        .collect();
    let groups: BTreeSet<(String, String)> = placed
        .keys()
        .filter(|(_, item, _)| *item == "group")
        .map(|(root, _, key)| (root.clone(), key.clone()))
        .collect();
    assert_eq!(seats, listed);
    assert_eq!(groups, headers);
    // The fixture reaches every state the rules tell apart.
    let states: BTreeSet<Option<String>> = seats.values().map(|seat| seat.1.clone()).collect();
    assert_eq!(states.len(), 10, "{states:?}");
    let reasons: BTreeSet<Option<String>> = seats.values().map(|seat| seat.2.clone()).collect();
    assert_eq!(reasons.len(), 6, "{reasons:?}");
}

fn tab_name(tab: TriageTab) -> &'static str {
    match tab {
        TriageTab::Pending => "pending",
        TriageTab::Done => "done",
        TriageTab::Skipped => "skipped",
    }
}

fn state_name(state: PendingState) -> &'static str {
    match state {
        PendingState::NotLookedUp => "not_looked_up",
        PendingState::Identifying => "identifying",
        PendingState::NeedsYou => "needs_you",
        PendingState::Identified => "identified",
        PendingState::Unmatched => "unmatched",
        PendingState::LookupError => "lookup_error",
        PendingState::Error => "error",
        PendingState::Importing => "importing",
        PendingState::ImportError => "import_error",
    }
}

fn reason(standing: &crate::import::PendingStanding) -> Option<NeedsYouReason> {
    match standing {
        crate::import::PendingStanding::NeedsYou { reason } => Some(*reason),
        _ => None,
    }
}

fn reason_name(reason: NeedsYouReason) -> &'static str {
    match reason {
        NeedsYouReason::Matches { .. } => "matches",
        NeedsYouReason::NoTracklist => "no_tracklist",
        NeedsYouReason::MediumMismatch { .. } => "medium_mismatch",
        NeedsYouReason::NotFound => "not_found",
        NeedsYouReason::NothingToLookUp => "nothing_to_look_up",
    }
}
