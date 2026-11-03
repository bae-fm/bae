use super::*;

#[test]
fn round_trips_each_kind() {
    for id in [
        SubId::Artist("a1".to_string()),
        SubId::Album("r1".to_string()),
        SubId::Track("t1".to_string()),
    ] {
        assert_eq!(SubId::parse(&id.encode()).unwrap(), id);
    }
}

#[test]
fn a_prefixless_id_is_not_found() {
    assert_eq!(SubId::parse("plain-uuid").unwrap_err().code, 70);
    assert_eq!(SubId::parse("").unwrap_err().code, 70);
    assert_eq!(SubId::parse("ar-").unwrap_err().code, 70);
}

#[test]
fn wrong_kind_is_not_found() {
    let album = SubId::parse("al-r1").unwrap();
    assert_eq!(album.expect_artist().unwrap_err().code, 70);
    assert_eq!(album.expect_track().unwrap_err().code, 70);
    assert_eq!(album.expect_album().unwrap(), "r1");
}

#[test]
fn cover_art_ids_carry_their_version_and_parse_to_their_subject() {
    let album = SubId::Album("r1".to_string());
    let id = album.cover_art_id("v2");
    assert_eq!(id, "al-r1~v2");
    assert_eq!(SubId::parse_cover_art(&id).unwrap(), album);
    assert_eq!(SubId::parse_cover_art("al-r1").unwrap(), album);
    assert_eq!(SubId::parse_cover_art("al-r1~").unwrap_err().code, 70);
}
