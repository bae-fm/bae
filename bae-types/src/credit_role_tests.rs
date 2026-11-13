use super::*;

#[test]
fn the_writing_roles_are_the_first_eight() {
    let writing: Vec<_> = CreditRoleKind::writing().map(CreditRoleKind::key).collect();
    assert_eq!(
        writing,
        [
            "writer",
            "composer",
            "lyricist",
            "librettist",
            "translator",
            "reviser",
            "scriptwriter",
            "reconstructor"
        ]
    );
    for kind in [
        CreditRoleKind::Arranger,
        CreditRoleKind::Producer,
        CreditRoleKind::Engineer,
        CreditRoleKind::Mixer,
        CreditRoleKind::Mastering,
    ] {
        assert!(!kind.is_writing(), "{kind:?}");
    }
}

#[test]
fn the_writing_keys_stand_in_a_sql_list() {
    assert_eq!(
        CreditRoleKind::writing_keys_sql(),
        "'writer', 'composer', 'lyricist', 'librettist', 'translator', 'reviser', \
         'scriptwriter', 'reconstructor'"
    );
}

#[test]
fn a_credit_is_what_its_columns_hold() {
    let named = CreditedAs {
        role: CreditRole::Kind(CreditRoleKind::Arranger),
        qualifiers: vec![
            RoleQualifier::Kind(QualifierKind::Co),
            RoleQualifier::Stated("lead vocals".to_string()),
        ],
    };
    let columns = named.columns();
    assert_eq!(columns.role, "arranger");
    assert_eq!(columns.role_words, None);
    assert_eq!(columns.qualifiers, r#"[{"kind":"co"},{"stated":"lead vocals"}]"#);
    assert_eq!(
        CreditedAs::from_columns(columns.role, columns.role_words, &columns.qualifiers).unwrap(),
        named
    );

    let stated = CreditedAs { role: CreditRole::Stated("adapter".to_string()), qualifiers: vec![] };
    let columns = stated.columns();
    assert_eq!((columns.role, columns.role_words.as_deref()), ("stated", Some("adapter")));
    assert_eq!(columns.qualifiers, "[]");
    assert_eq!(
        CreditedAs::from_columns(columns.role, columns.role_words, &columns.qualifiers).unwrap(),
        stated
    );
}

#[test]
fn a_performer_holds_its_part_in_the_words_column() {
    for part in [Some("bass guitar".to_string()), None] {
        let performer = CreditedAs {
            role: CreditRole::Performer(part.clone()),
            qualifiers: vec![RoleQualifier::Kind(QualifierKind::Guest)],
        };
        let columns = performer.columns();
        assert_eq!((columns.role, columns.role_words.clone()), ("performer", part));
        assert_eq!(
            CreditedAs::from_columns(columns.role, columns.role_words, &columns.qualifiers)
                .unwrap(),
            performer
        );
    }
}

#[test]
fn columns_that_contradict_each_other_are_refused() {
    assert!(matches!(
        CreditedAs::from_columns("stated", None, "[]"),
        Err(CreditColumnsError::StatedWithoutWords)
    ));
    assert!(matches!(
        CreditedAs::from_columns("composer", Some("words".to_string()), "[]"),
        Err(CreditColumnsError::WordsWithKey(_))
    ));
    assert!(matches!(
        CreditedAs::from_columns("conductor", None, "[]"),
        Err(CreditColumnsError::UnknownRole(_))
    ));
    assert!(matches!(
        CreditedAs::from_columns("composer", None, "[\"co\"]"),
        Err(CreditColumnsError::Qualifiers(..))
    ));
}
