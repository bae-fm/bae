use super::*;

fn recorded(page: &str) -> Vec<(&str, &str)> {
    page.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split_once('\t').expect("a heading and a role"))
        .collect()
}

#[test]
fn the_table_is_discogs_s_list() {
    assert_eq!(
        ROLES.iter().map(|(heading, role, _)| (*heading, *role)).collect::<Vec<_>>(),
        recorded(include_str!("../../test-fixtures/credit-vocabulary/discogs-credits.txt"))
    );
}

fn kind(kind: CreditRoleKind, qualifiers: Vec<RoleQualifier>) -> CreditedAs {
    CreditedAs { role: CreditRole::Kind(kind), qualifiers }
}

fn instrument(name: &str) -> CreditedAs {
    let instrument = Instrument::from_name(name).expect("an instrument on the list");
    CreditedAs {
        role: CreditRole::Performer(Some(Part::Instrument(instrument))),
        qualifiers: vec![],
    }
}

fn qualified(credit: CreditedAs, qualifiers: Vec<RoleQualifier>) -> CreditedAs {
    CreditedAs { qualifiers, ..credit }
}

fn voice(name: &str) -> CreditedAs {
    let voice = Voice::from_name(name).expect("a voice on the list");
    CreditedAs { role: CreditRole::Performer(Some(Part::Voice(voice))), qualifiers: vec![] }
}

fn named(words: &str) -> CreditedAs {
    CreditedAs {
        role: CreditRole::Performer(Some(Part::Words(words.to_string()))),
        qualifiers: vec![],
    }
}

#[test]
fn a_string_of_roles_is_a_credit_for_each() {
    assert_eq!(credits("Guitar, Vocals"), [instrument("guitar"), voice("vocal")]);
    assert_eq!(credits("Producer"), [kind(CreditRoleKind::Producer, vec![])]);
    assert!(credits("").is_empty());
    assert!(credits(" , ").is_empty());
}

#[test]
fn a_bracket_qualifies_its_role_and_its_commas_do_not_split() {
    assert_eq!(
        credits("Engineer [Assistant]"),
        [kind(CreditRoleKind::Engineer, vec![RoleQualifier::Kind(QualifierKind::Assistant)])]
    );
    assert_eq!(
        credits("Engineer [Assistant, Mixing]"),
        [kind(
            CreditRoleKind::Engineer,
            vec![RoleQualifier::Stated("Assistant, Mixing".to_string())]
        )]
    );
    assert_eq!(
        credits("Piano [Baldwin Acoustic SF-10], Electric Piano [Fender Rhodes], Organ [Hammond B-3], Vocals"),
        [
            qualified(
                instrument("piano"),
                vec![RoleQualifier::Stated("Baldwin Acoustic SF-10".to_string())]
            ),
            qualified(
                instrument("electric piano"),
                vec![RoleQualifier::Stated("Fender Rhodes".to_string())]
            ),
            qualified(instrument("organ"), vec![RoleQualifier::Stated("Hammond B-3".to_string())]),
            voice("vocal"),
        ]
    );
}

#[test]
fn a_name_that_states_a_modifier_reads_it_as_the_qualifier() {
    assert_eq!(
        credits("Co-producer"),
        [kind(CreditRoleKind::Producer, vec![RoleQualifier::Kind(QualifierKind::Co)])]
    );
    assert_eq!(
        credits("Executive-Producer, Executive Producer"),
        [
            kind(CreditRoleKind::Producer, vec![RoleQualifier::Kind(QualifierKind::Executive)]),
            kind(CreditRoleKind::Producer, vec![RoleQualifier::Kind(QualifierKind::Executive)]),
        ]
    );
    assert_eq!(
        credits("Remastered By"),
        [kind(CreditRoleKind::Mastering, vec![RoleQualifier::Kind(QualifierKind::Re)])]
    );
}

#[test]
fn the_writing_names_read_as_the_writing_roles() {
    for (name, role) in [
        ("Written-By", CreditRoleKind::Writer),
        ("Written By", CreditRoleKind::Writer),
        ("Songwriter", CreditRoleKind::Writer),
        ("Composed By", CreditRoleKind::Composer),
        ("Music By", CreditRoleKind::Composer),
        ("Lyrics By", CreditRoleKind::Lyricist),
        ("Words By", CreditRoleKind::Lyricist),
        ("Libretto By", CreditRoleKind::Librettist),
        ("Translated By", CreditRoleKind::Translator),
        ("Script By", CreditRoleKind::Scriptwriter),
        ("Arranged By", CreditRoleKind::Arranger),
        ("Mixed By", CreditRoleKind::Mixer),
        ("Mastered By", CreditRoleKind::Mastering),
    ] {
        assert_eq!(credits(name), [kind(role, vec![])], "{name}");
    }
}

#[test]
fn a_performer_names_a_part_unless_the_role_is_the_bare_one() {
    for name in ["Performer", "Musician"] {
        assert_eq!(
            credits(name),
            [CreditedAs { role: CreditRole::Performer(None), qualifiers: vec![] }]
        );
    }
    assert_eq!(credits("Backing Vocals"), [voice("background vocals")]);
}

/// Each instrument Discogs lists that MusicBrainz's list also holds is read as
/// MusicBrainz's instrument; one it has no instrument for stays in Discogs's
/// word.
#[test]
fn a_discogs_instrument_is_read_onto_musicbrainz_s_list() {
    for (name, listed) in [
        ("Bass", "bass guitar"),
        ("Drums", "drums (drum set)"),
        ("Percussion", "percussion"),
        ("Guitar", "guitar"),
        ("Keyboards", "keyboard"),
        ("Strings", "strings"),
        ("Violin", "violin"),
        ("Piano", "piano"),
        ("Electric Piano", "electric piano"),
        ("Organ", "organ"),
    ] {
        assert_eq!(credits(name), [instrument(listed)], "{name}");
    }
    for name in ["Orchestra", "Band", "Ensemble", "Soloist"] {
        assert_eq!(credits(name), [named(name)], "{name}");
    }
    for (_, name, reading) in ROLES {
        match reading {
            Reading::Instrument(listed) => {
                assert!(Instrument::from_name(listed).is_some(), "{name} reads onto {listed}");
            }
            Reading::Voice(listed) => {
                assert!(Voice::from_name(listed).is_some(), "{name} reads onto {listed}");
            }
            _ => {}
        }
    }
}

#[test]
fn any_other_role_is_credited_as_discogs_writes_it() {
    assert_eq!(
        credits("Recorded By"),
        [CreditedAs { role: CreditRole::Stated("Recorded By".to_string()), qualifiers: vec![] }]
    );
    assert_eq!(
        credits("Design [Album Design]"),
        [CreditedAs {
            role: CreditRole::Stated("Design".to_string()),
            qualifiers: vec![RoleQualifier::Stated("Album Design".to_string())]
        }]
    );
    assert_eq!(
        credits("Tin Whistle"),
        [CreditedAs { role: CreditRole::Stated("Tin Whistle".to_string()), qualifiers: vec![] }]
    );
}

#[test]
fn names_compare_with_case_aside() {
    assert_eq!(credits("written-by"), [kind(CreditRoleKind::Writer, vec![])]);
}
