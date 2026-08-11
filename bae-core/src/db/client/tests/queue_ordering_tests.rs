use super::super::*;
use crate::playback::QueueEntryId;
use std::collections::HashMap;

fn meta(id: &str) -> TrackQueueMeta {
    TrackQueueMeta {
        title: format!("Title {id}"),
        artist_names: "Artist Name".to_string(),
        duration_ms: Some(1000),
        album_title: "Album Title".to_string(),
        cover_image: Some(crate::album_detail::ImageRef {
            id: format!("rel-{id}"),
            version: format!("stamp-{id}"),
            image_type: LibraryImageType::Cover,
        }),
    }
}

fn entry(entry_id: &str, track_id: &str) -> QueueEntry {
    QueueEntry {
        id: QueueEntryId(entry_id.to_string()),
        track_id: track_id.to_string(),
    }
}

#[test]
fn preserves_duplicate_queue_entries_in_order_with_distinct_ids() {
    let mut meta_by_track = HashMap::new();
    meta_by_track.insert("a".to_string(), meta("a"));
    meta_by_track.insert("b".to_string(), meta("b"));

    // The same track queued twice resolves twice, in position order, each
    // carrying its own entry id.
    let resolved = resolve_queue_entries(
        &meta_by_track,
        &[entry("e0", "a"), entry("e1", "a"), entry("e2", "b")],
    );

    let track_ids: Vec<&str> = resolved.iter().map(|i| i.track_id.as_str()).collect();
    assert_eq!(track_ids, vec!["a", "a", "b"]);
    let entry_ids: Vec<&str> = resolved.iter().map(|i| i.entry_id.as_str()).collect();
    assert_eq!(entry_ids, vec!["e0", "e1", "e2"]);
}

#[test]
fn skips_entries_whose_track_is_unknown() {
    let mut meta_by_track = HashMap::new();
    meta_by_track.insert("a".to_string(), meta("a"));
    let resolved =
        resolve_queue_entries(&meta_by_track, &[entry("e0", "a"), entry("e1", "missing")]);
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].entry_id, "e0");
}
