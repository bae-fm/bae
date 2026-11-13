use super::*;

fn recorded(page: &str) -> Vec<(&str, &str)> {
    page.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut columns = line.split('\t');
            (columns.next().unwrap(), columns.next().unwrap())
        })
        .collect()
}

#[test]
fn each_table_is_musicbrainzs_own_list() {
    for (table, page) in [
        (
            ARTIST_WORK,
            include_str!("../../test-fixtures/credit-vocabulary/musicbrainz-artist-work.txt"),
        ),
        (
            ARTIST_RECORDING,
            include_str!("../../test-fixtures/credit-vocabulary/musicbrainz-artist-recording.txt"),
        ),
        (
            ARTIST_RELEASE,
            include_str!("../../test-fixtures/credit-vocabulary/musicbrainz-artist-release.txt"),
        ),
    ] {
        assert_eq!(
            table.iter().map(|(name, phrase, _)| (*name, *phrase)).collect::<Vec<_>>(),
            recorded(page)
        );
    }
}

fn read(relation_type: &str, names: &[&str], credits: &[(&str, &str)]) -> Reading {
    read_valued(Subject::Work, relation_type, names, credits, &[])
}

fn read_valued(
    subject: Subject,
    relation_type: &str,
    names: &[&str],
    credits: &[(&str, &str)],
    values: &[(&str, &str)],
) -> Reading {
    read_with_ids(subject, relation_type, names, &[], credits, values)
}

/// A relation whose attributes carry the MusicBrainz ids `ids` pairs with
/// their names.
fn read_with_ids(
    subject: Subject,
    relation_type: &str,
    names: &[&str],
    ids: &[(&str, &str)],
    credits: &[(&str, &str)],
    values: &[(&str, &str)],
) -> Reading {
    let names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
    let map = |pairs: &[(&str, &str)]| {
        pairs.iter().map(|(key, value)| (key.to_string(), value.to_string())).collect()
    };
    super::credits(
        subject,
        relation_type,
        Attributes { names: &names, ids: &map(ids), credits: &map(credits), values: &map(values) },
    )
}

fn credit(role: CreditRole, qualifiers: Vec<RoleQualifier>) -> Reading {
    Reading::Credits(vec![CreditedAs { role, qualifiers }])
}

#[test]
fn the_writing_family_reads_as_bae_s_writing_roles() {
    for (relation, kind) in [
        ("writer", CreditRoleKind::Writer),
        ("composer", CreditRoleKind::Composer),
        ("lyricist", CreditRoleKind::Lyricist),
        ("librettist", CreditRoleKind::Librettist),
        ("translator", CreditRoleKind::Translator),
        ("revised by", CreditRoleKind::Reviser),
        ("scriptwriter", CreditRoleKind::Scriptwriter),
        ("reconstructed by", CreditRoleKind::Reconstructor),
    ] {
        assert_eq!(read(relation, &[], &[]), credit(CreditRole::Kind(kind), vec![]));
        assert!(kind.is_writing());
    }
}

#[test]
fn an_arranger_of_any_kind_is_an_arranger_with_what_it_arranged_for() {
    let arranger = CreditRole::Kind(CreditRoleKind::Arranger);
    assert_eq!(
        read("vocal arranger", &["lead vocals"], &[]),
        credit(arranger.clone(), vec![RoleQualifier::Stated("lead vocals".to_string())])
    );
    assert_eq!(
        read("vocal arranger", &[], &[]),
        credit(arranger.clone(), vec![RoleQualifier::Stated("vocals".to_string())])
    );
    assert_eq!(
        read("instrument arranger", &["additional", "cello"], &[("cello", "violoncello")]),
        credit(
            arranger.clone(),
            vec![
                RoleQualifier::Kind(QualifierKind::Additional),
                RoleQualifier::Stated("violoncello".to_string())
            ]
        )
    );
    assert_eq!(
        read("arranger", &["co", "associate"], &[]),
        credit(
            arranger,
            vec![
                RoleQualifier::Kind(QualifierKind::Co),
                RoleQualifier::Kind(QualifierKind::Associate)
            ]
        )
    );
}

#[test]
fn a_role_bae_has_no_name_for_is_stated_in_the_phrase() {
    assert_eq!(
        read("adapter", &[], &[]),
        credit(CreditRole::Stated("adapter".to_string()), vec![])
    );
    assert_eq!(
        read("publishing", &["sub"], &[]),
        credit(
            CreditRole::Stated("publisher".to_string()),
            vec![RoleQualifier::Stated("sub".to_string())]
        )
    );
    assert_eq!(
        read("orchestrator", &[], &[]),
        credit(CreditRole::Stated("orchestrator".to_string()), vec![])
    );
}

#[test]
fn a_task_is_a_qualifier_in_its_own_words() {
    assert_eq!(
        read_valued(Subject::Work, "choreographer", &["task"], &[], &[("task", "fight scenes")]),
        credit(
            CreditRole::Stated("choreographer".to_string()),
            vec![RoleQualifier::Stated("fight scenes".to_string())]
        )
    );
}

#[test]
fn a_type_naming_an_artist_without_crediting_work_is_not_a_credit() {
    for relation in [
        "previous attribution",
        "commissioned",
        "dedication",
        "premiere",
        "named after artist",
        "named after work",
    ] {
        assert_eq!(read(relation, &[], &[]), Reading::NotACredit, "{relation}");
    }
}

#[test]
fn a_type_the_page_does_not_list_is_credited_in_its_own_name() {
    assert_eq!(
        read("conjurer", &[], &[]),
        Reading::Unlisted(CreditedAs {
            role: CreditRole::Stated("conjurer".to_string()),
            qualifiers: vec![]
        })
    );
}

#[test]
fn a_phrase_reads_as_words_without_its_placeholders() {
    assert_eq!(phrase_words("{additional} {associate} {co:co-}arranger"), "arranger");
    assert_eq!(phrase_words("{co:co-}{executive:executive }producer"), "producer");
    assert_eq!(
        phrase_words("{additional} {assistant} {associate} {instrument} programming"),
        "programming"
    );
    assert_eq!(phrase_words("{instrument:%|instruments} technician"), "instruments technician");
    assert_eq!(phrase_words("DJ-mixer {medium}"), "DJ-mixer");
    assert_eq!(phrase_words("phonographic copyright (℗) by"), "phonographic copyright (℗) by");
}

#[test]
fn a_recording_and_a_release_state_the_roles_bae_names_beyond_writing() {
    for (relation, kind) in [
        ("producer", CreditRoleKind::Producer),
        ("engineer", CreditRoleKind::Engineer),
        ("mix", CreditRoleKind::Mixer),
        ("mastering", CreditRoleKind::Mastering),
        ("arranger", CreditRoleKind::Arranger),
    ] {
        for subject in [Subject::Recording, Subject::Release] {
            assert_eq!(
                read_valued(subject, relation, &[], &[], &[]),
                credit(CreditRole::Kind(kind), vec![]),
                "{relation}"
            );
        }
    }
    assert_eq!(
        read_valued(Subject::Recording, "engineer", &["assistant"], &[], &[]),
        credit(
            CreditRole::Kind(CreditRoleKind::Engineer),
            vec![RoleQualifier::Kind(QualifierKind::Assistant)]
        )
    );
    assert_eq!(
        read_valued(Subject::Release, "mastering", &["additional", "re"], &[], &[]),
        credit(
            CreditRole::Kind(CreditRoleKind::Mastering),
            vec![
                RoleQualifier::Kind(QualifierKind::Additional),
                RoleQualifier::Kind(QualifierKind::Re)
            ]
        )
    );
}

#[test]
fn a_recording_engineer_is_stated_in_the_phrase() {
    assert_eq!(
        read_valued(Subject::Recording, "recording", &["assistant"], &[], &[]),
        credit(
            CreditRole::Stated("recording engineer".to_string()),
            vec![RoleQualifier::Kind(QualifierKind::Assistant)]
        )
    );
    assert_eq!(
        read_valued(Subject::Release, "mix-DJ", &[], &[], &[]),
        credit(CreditRole::Stated("DJ-mixer".to_string()), vec![])
    );
    assert_eq!(
        read_valued(Subject::Recording, "performing orchestra", &[], &[], &[]),
        credit(CreditRole::Stated("orchestra".to_string()), vec![])
    );
}

fn performer(part: Option<Part>, qualifiers: Vec<RoleQualifier>) -> CreditedAs {
    CreditedAs { role: CreditRole::Performer(part), qualifiers }
}

const KEYBOARD: &str = "95b0c3d2-9606-4ef5-a019-9b7437f3adda";
const SYNTHESIZER: &str = "4a29230c-5ab5-4eff-ac59-4a253f3561a0";
const DRUMS: &str = "12092505-6ee1-46af-a15a-b5b468b6b155";

fn listed(id: &str) -> Option<Part> {
    Some(Part::Instrument(Instrument::from_id(id).expect("an instrument on the list")))
}

#[test]
fn an_instrument_relation_is_a_performer_for_each_instrument_it_names() {
    assert_eq!(
        read_with_ids(
            Subject::Recording,
            "instrument",
            &["keyboard", "synthesizer"],
            &[("keyboard", KEYBOARD), ("synthesizer", SYNTHESIZER)],
            &[],
            &[]
        ),
        Reading::Credits(vec![
            performer(listed(KEYBOARD), vec![]),
            performer(listed(SYNTHESIZER), vec![])
        ])
    );
    assert_eq!(
        read_with_ids(
            Subject::Recording,
            "instrument",
            &["additional", "drums (drum set)", "guest"],
            &[("drums (drum set)", DRUMS)],
            &[("drums (drum set)", "drums")],
            &[]
        ),
        Reading::Credits(vec![performer(
            listed(DRUMS),
            vec![
                RoleQualifier::Kind(QualifierKind::Additional),
                RoleQualifier::Kind(QualifierKind::Guest)
            ]
        )])
    );
    assert_eq!(
        read_valued(Subject::Release, "instrument", &[], &[], &[]),
        Reading::Credits(vec![performer(None, vec![])])
    );
}

/// An instrument MusicBrainz's list does not hold, or one its relation gives
/// no id for, is shown as the credit prints it.
#[test]
fn an_instrument_the_list_does_not_hold_is_the_credit_s_words() {
    let words = |text: &str| Some(Part::Words(text.to_string()));
    assert_eq!(
        read_with_ids(
            Subject::Recording,
            "instrument",
            &["newly invented zither"],
            &[("newly invented zither", "00000000-0000-4000-8000-000000000000")],
            &[],
            &[]
        ),
        Reading::Credits(vec![performer(words("newly invented zither"), vec![])])
    );
    assert_eq!(
        read_valued(
            Subject::Recording,
            "instrument",
            &["drums (drum set)"],
            &[("drums (drum set)", "drums")],
            &[]
        ),
        Reading::Credits(vec![performer(words("drums"), vec![])])
    );
}

#[test]
fn a_vocal_relation_is_a_performer_for_each_voice_and_vocals_for_none() {
    const LEAD: &str = "8e2a3255-87c2-4809-a174-98cb3704f1a5";
    assert_eq!(
        read_with_ids(
            Subject::Recording,
            "vocal",
            &["lead vocals", "solo"],
            &[("lead vocals", LEAD)],
            &[],
            &[]
        ),
        Reading::Credits(vec![performer(
            Some(Part::Voice(Voice::from_id(LEAD).unwrap())),
            vec![RoleQualifier::Kind(QualifierKind::Solo)]
        )])
    );
    assert_eq!(
        read_valued(Subject::Recording, "vocal", &[], &[], &[]),
        Reading::Credits(vec![performer(Some(Part::Voice(Voice::general())), vec![])])
    );
    assert_eq!(
        read_valued(Subject::Recording, "vocal", &["yodelling vocals"], &[], &[]),
        Reading::Credits(vec![performer(
            Some(Part::Words("yodelling vocals".to_string())),
            vec![]
        )])
    );
}

#[test]
fn a_performer_relation_names_no_instrument() {
    assert_eq!(
        read_valued(Subject::Recording, "performer", &["guest"], &[], &[]),
        Reading::Credits(vec![performer(None, vec![RoleQualifier::Kind(QualifierKind::Guest)])])
    );
}

#[test]
fn a_sample_credit_is_no_credit_of_the_recording() {
    assert_eq!(
        read_valued(Subject::Recording, "samples from artist", &[], &[], &[]),
        Reading::NotACredit
    );
}
