use super::*;
use bae_types::credit_role::CreditRoleKind;

fn positions(count: usize) -> Vec<String> {
    (1..=count).map(|number| number.to_string()).collect()
}

fn tracks(count: usize) -> Vec<String> {
    (0..count).map(|index| format!("track-{index}")).collect()
}

fn rows(credits: &[DbReleaseCredit], count: usize) -> Vec<CreditRow> {
    let ids = tracks(count);
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    credit_rows(credits, &ids, &positions(count))
}

/// A credit on the track at index `index`.
fn on(index: usize) -> DbCreditScope {
    DbCreditScope::Track(format!("track-{index}"))
}

fn credit(artist: &str, role: CreditRoleKind, scope: DbCreditScope) -> DbReleaseCredit {
    DbReleaseCredit {
        artist_id: artist.to_string(),
        printed: format!("{artist} printed"),
        role: CreditedAs::of(role),
        scope,
    }
}

fn names(row: &CreditRow) -> Vec<&str> {
    row.artists.names().iter().map(|credited| credited.name.as_str()).collect()
}

#[test]
fn the_release_s_credits_come_first_then_each_track_s_by_track() {
    let credits = [
        credit("producer-a", CreditRoleKind::Producer, DbCreditScope::Release),
        credit("engineer-a", CreditRoleKind::Engineer, DbCreditScope::Release),
        credit("arranger-a", CreditRoleKind::Arranger, on(1)),
        credit("arranger-a", CreditRoleKind::Arranger, on(4)),
    ];

    let rows = rows(&credits, 6);

    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].role, CreditedAs::of(CreditRoleKind::Producer));
    assert!(rows[0].tracks.is_empty());
    assert_eq!(rows[1].role, CreditedAs::of(CreditRoleKind::Engineer));
    assert_eq!(rows[2].role, CreditedAs::of(CreditRoleKind::Arranger));
    assert_eq!(rows[2].tracks, ["2", "5"]);
}

#[test]
fn artists_sharing_a_role_and_the_same_tracks_are_one_row() {
    let credits = [
        credit("producer-a", CreditRoleKind::Producer, DbCreditScope::Release),
        credit("producer-b", CreditRoleKind::Producer, DbCreditScope::Release),
        credit("producer-c", CreditRoleKind::Producer, on(0)),
    ];

    let rows = rows(&credits, 3);

    assert_eq!(rows.len(), 2);
    assert_eq!(names(&rows[0]), ["producer-a printed", "producer-b printed"]);
    assert!(rows[0].tracks.is_empty());
    assert_eq!(names(&rows[1]), ["producer-c printed"]);
    assert_eq!(rows[1].tracks, ["1"]);
}

#[test]
fn a_credit_on_every_track_prints_no_tracks() {
    let credits = [
        credit("mixer-a", CreditRoleKind::Mixer, on(0)),
        credit("mixer-a", CreditRoleKind::Mixer, on(1)),
    ];

    let rows = rows(&credits, 2);

    assert_eq!(rows.len(), 1);
    assert!(rows[0].tracks.is_empty());
}

#[test]
fn one_artist_in_two_roles_is_two_rows() {
    let credits = [
        credit("artist-a", CreditRoleKind::Producer, DbCreditScope::Release),
        credit("artist-a", CreditRoleKind::Engineer, DbCreditScope::Release),
    ];

    let rows = rows(&credits, 2);

    assert_eq!(rows.len(), 2);
    assert_eq!(names(&rows[0]), names(&rows[1]));
}

#[test]
fn a_release_with_no_credit_has_no_row() {
    assert!(rows(&[], 3).is_empty());
}

#[test]
fn qualifiers_keep_roles_apart() {
    let mut assistant = credit("engineer-a", CreditRoleKind::Engineer, DbCreditScope::Release);
    assistant.role.qualifiers =
        vec![bae_types::credit_role::RoleQualifier::Stated("assistant".to_string())];
    let credits =
        [assistant, credit("engineer-b", CreditRoleKind::Engineer, DbCreditScope::Release)];

    assert_eq!(rows(&credits, 2).len(), 2);
}
