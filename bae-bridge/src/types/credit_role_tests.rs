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
