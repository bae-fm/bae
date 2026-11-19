use super::*;

/// A join or a pairing given another kind of code fails with coven's refusal
/// of the code's kind, before any sign-in and before anything connects.
#[tokio::test]
async fn a_join_or_a_pairing_refuses_a_restore_code() {
    let temp = bae_util::test_files::TestFolder::new();
    let app_dir = AppDir::at(temp.path()).unwrap();
    let installation = coven::Coven::in_memory(app_dir.coven_app_dir());
    let storage = std::sync::Arc::new(coven::MemoryStorage::builder().build().unwrap());
    let (_, _, code) = connected_library(&installation, &storage, "Household").await;
    let clock: coven::ClockRef = std::sync::Arc::new(coven::SystemClock);
    let wrong_kind = |error: RestoreFromCodeError| match error {
        RestoreFromCodeError::Restore(error) => match *error {
            coven::BootstrapError::Code(coven::CodeError::WrongKind { expected, actual }) => {
                (expected, actual)
            }
            error => panic!("refused for another reason: {error:?}"),
        },
        error => panic!("refused for another reason: {error:?}"),
    };

    let joined = join_with_invite(
        bae_store::test_devices::test_builder(&installation),
        &code,
        bae_store::EnteredName::parse("Rowan").unwrap(),
        "Guest Device",
        clock.clone(),
        |_| {},
        &JoinControl::new(),
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await;
    let Err(error) = joined else { panic!("a restore code joined a library") };
    assert_eq!(wrong_kind(error), (coven::CodeKind::Invite, coven::CodeKind::Restore));

    let paired = join_with_pairing(
        bae_store::test_devices::test_builder(&installation),
        &code,
        Some(owner_key()),
        "Phone",
        clock,
        |_| {},
        &JoinControl::new(),
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await;
    let Err(error) = paired else { panic!("a restore code paired a device") };
    assert_eq!(wrong_kind(error), (coven::CodeKind::Pairing, coven::CodeKind::Restore));
}
