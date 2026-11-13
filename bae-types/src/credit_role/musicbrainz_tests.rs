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
fn the_artist_work_table_is_musicbrainzs_own_list() {
    assert_eq!(
        ARTIST_WORK.iter().map(|(name, phrase, _)| (*name, *phrase)).collect::<Vec<_>>(),
        recorded(include_str!("../../test-fixtures/credit-vocabulary/musicbrainz-artist-work.txt"))
    );
}

fn read(relation_type: &str, names: &[&str], credits: &[(&str, &str)]) -> WorkReading {
    read_valued(relation_type, names, credits, &[])
}

fn read_valued(
    relation_type: &str,
    names: &[&str],
    credits: &[(&str, &str)],
    values: &[(&str, &str)],
) -> WorkReading {
    let names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
    let map = |pairs: &[(&str, &str)]| {
        pairs.iter().map(|(key, value)| (key.to_string(), value.to_string())).collect()
    };
    work_credit(
        relation_type,
        Attributes { names: &names, credits: &map(credits), values: &map(values) },
    )
}

fn credit(role: CreditRole, qualifiers: Vec<RoleQualifier>) -> WorkReading {
    WorkReading::Credit(CreditedAs { role, qualifiers })
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
        read_valued("choreographer", &["task"], &[], &[("task", "fight scenes")]),
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
        assert_eq!(read(relation, &[], &[]), WorkReading::NotACredit, "{relation}");
    }
}

#[test]
fn a_type_the_page_does_not_list_is_credited_in_its_own_name() {
    assert_eq!(
        read("conjurer", &[], &[]),
        WorkReading::Unlisted(CreditedAs {
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
