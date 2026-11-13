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
        recorded(include_str!("discogs-credits.tsv"))
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
    for name in ["Orchestra", "Band", "Ensemble"] {
        assert_eq!(credits(name), [named(name)], "{name}");
    }
    for (_, name, reading) in ROLES.iter() {
        match reading {
            Reading::Instrument(listed) => {
                assert!(Instrument::from_name(listed).is_some(), "{name} reads onto {listed}");
            }
            Reading::Voice(listed, _) => {
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
        credits("Tin Kettle"),
        [CreditedAs { role: CreditRole::Stated("Tin Kettle".to_string()), qualifiers: vec![] }]
    );
}

#[test]
fn names_compare_with_case_aside() {
    assert_eq!(credits("written-by"), [kind(CreditRoleKind::Writer, vec![])]);
}

/// A role the capture lists is read as exactly one credit, in the kind of
/// credit its table row states.
#[test]
fn every_captured_role_is_read_or_kept_by_name() {
    let captured = recorded(include_str!("discogs-credits.tsv"));
    assert_eq!(captured.len(), 1078);
    for (heading, role) in captured {
        let row = ROLES
            .iter()
            .find(|(listed_heading, listed, _)| *listed_heading == heading && *listed == role)
            .unwrap_or_else(|| panic!("{role} under {heading} has a row"));
        let read = credits(role);
        assert_eq!(read.len(), 1, "{role}");
        match (row.2, &read[0].role) {
            (Reading::Stated, CreditRole::Stated(words)) => assert_eq!(words, role),
            (Reading::Named, CreditRole::Performer(Some(Part::Words(words)))) => {
                assert_eq!(words, role)
            }
            (Reading::Stated | Reading::Named, other) => {
                panic!("{role} is kept by name but reads as {other:?}")
            }
            (_, CreditRole::Stated(_)) => panic!("{role} has a reading but is kept by name"),
            _ => {}
        }
    }
}

#[test]
fn an_instrument_discogs_lists_is_the_one_musicbrainz_lists() {
    for (name, listed) in [
        ("Cello", "cello"),
        ("Acoustic Guitar", "acoustic guitar"),
        ("Snare", "snare drum"),
        ("English Horn", "cor anglais"),
        ("Violoncello", "cello"),
        ("Tin Whistle", "tin whistle"),
        ("Ondes Martenot", "ondes Martenot"),
    ] {
        assert_eq!(credits(name), [instrument(listed)], "{name}");
    }
}

#[test]
fn a_vocal_role_discogs_lists_is_the_voice_musicbrainz_lists() {
    for (name, listed) in [
        ("Lead Vocals", "lead vocals"),
        ("Soprano Vocals", "soprano vocals"),
        ("Choir", "choir vocals"),
        ("Speech", "spoken vocals"),
        ("Whistling", "whistling"),
        ("Voice", "vocal"),
    ] {
        assert_eq!(credits(name), [voice(listed)], "{name}");
    }
    assert_eq!(
        credits("Solo Vocal"),
        [qualified(voice("vocal"), vec![RoleQualifier::Kind(QualifierKind::Solo)])]
    );
}

#[test]
fn a_role_that_is_a_modifier_is_a_performer_with_that_qualifier() {
    let performer = CreditedAs { role: CreditRole::Performer(None), qualifiers: vec![] };
    assert_eq!(
        credits("Guest"),
        [qualified(performer.clone(), vec![RoleQualifier::Kind(QualifierKind::Guest)])]
    );
    assert_eq!(
        credits("Soloist"),
        [qualified(performer, vec![RoleQualifier::Kind(QualifierKind::Solo)])]
    );
}

#[test]
fn an_instrument_or_voice_bae_has_no_entry_for_keeps_discogs_s_word() {
    for name in ["Drum Programming", "Harmony Vocals", "Rap", "Hang Drum", "Lead Guitar"] {
        assert_eq!(credits(name), [named(name)], "{name}");
    }
}

#[test]
fn a_role_bae_has_no_name_for_is_kept_by_name() {
    for name in ["Hosted By", "Conductor", "Remix", "Design", "Narrator", "Tour Manager"] {
        assert_eq!(
            credits(name),
            [CreditedAs { role: CreditRole::Stated(name.to_string()), qualifiers: vec![] }],
            "{name}"
        );
    }
}
