use super::*;

#[test]
fn a_role_bae_names_is_worded_from_the_catalog_and_the_catalog_s_words_are_shown_as_written() {
    let engineer = BridgeCreditRole::from_core(CreditedAs {
        role: CreditRole::Kind(CreditRoleKind::Engineer),
        qualifiers: vec![
            RoleQualifier::Kind(QualifierKind::Assistant),
            RoleQualifier::Stated("Strings".to_string()),
        ],
    });
    assert_eq!(
        engineer,
        BridgeCreditRole {
            role: localized("core.credit.role.engineer"),
            qualifiers: vec![localized("core.credit.qualifier.assistant"), verbatim("Strings")],
        }
    );

    let stated = BridgeCreditRole::from_core(CreditedAs {
        role: CreditRole::Stated("recording engineer".to_string()),
        qualifiers: vec![],
    });
    assert_eq!(stated.role, verbatim("recording engineer"));
    assert!(stated.qualifiers.is_empty());
}

#[test]
fn a_performer_is_worded_by_its_part_or_as_a_performer() {
    let guitar = BridgeCreditRole::from_core(CreditedAs {
        role: CreditRole::Performer(Some("bass guitar".to_string())),
        qualifiers: vec![],
    });
    assert_eq!(guitar.role, verbatim("bass guitar"));
    let bare = BridgeCreditRole::from_core(CreditedAs {
        role: CreditRole::Performer(None),
        qualifiers: vec![],
    });
    assert_eq!(bare.role, localized("core.credit.role.performer"));
}

#[test]
fn a_credit_row_words_its_role_and_carries_its_artists_and_tracks() {
    let row = bae_views::album_detail::CreditRow {
        role: CreditedAs::of(CreditRoleKind::Arranger),
        artists: bae_types::credits::CreditLine::from_stored([
            ("Artist Name One", None),
            ("Artist Name Two", None),
        ]),
        tracks: vec!["2".to_string(), "5".to_string()],
    };

    let bridged = BridgeCreditRow::from_core(row);

    assert_eq!(bridged.role.role, localized("core.credit.role.arranger"));
    assert_eq!(
        bridged.artists.names.iter().map(|credited| credited.name.as_str()).collect::<Vec<_>>(),
        ["Artist Name One", "Artist Name Two"]
    );
    assert_eq!(bridged.tracks, ["2", "5"]);
}
