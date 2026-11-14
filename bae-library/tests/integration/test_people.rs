use super::dropbox_sign_in::DropboxSignIn;
use super::test_joined_library::loaded;
use bae_library::followed::value_matching;
use bae_library::test_libraries::{
    connect_storage, made_profile, made_profile_on, requested_profile, storage_library,
};
use bae_people::people::{HistorySharing, ProfileError};

mod avatars;
mod ratings;

/// The diagnostics these tests' bootstrap calls report to: none.
static NO_DIAGNOSTICS: std::sync::LazyLock<bae_diagnostics::Diagnostics> =
    std::sync::LazyLock::new(bae_diagnostics::Diagnostics::noop);

#[tokio::test]
async fn a_profile_is_idempotent_once_asked_for() {
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = library.people().clone();
    assert!(value_matching(&mut people.subscribe_profiles(bae_views::sort_language_on), |_| true)
        .await
        .is_empty());
    connect_storage(library.manager(), library.store(), &storage).await;
    let profile = made_profile(library.manager(), library.people(), "Robin").await;
    assert_eq!(profile.history_sharing, HistorySharing::Private);
    library.people().create_profile("Robin").await.unwrap();
    assert_eq!(people.profile(profile.id).await.unwrap(), Some(profile.clone()));
    assert!(matches!(
        library.people().create_profile("Another Name").await,
        Err(ProfileError::AlreadyExists)
    ));
    let listed = library.people().people().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].profile.as_ref(), Some(&profile));
    assert!(listed[0].membership.is_self);
    library.people().rename_profile("New Name").await.unwrap();
    library.people().set_history_sharing(HistorySharing::Household).await.unwrap();
    let profile = people.profile(profile.id).await.unwrap().unwrap();
    assert_eq!(profile.name, "New Name");
    assert_eq!(profile.history_sharing, HistorySharing::Household);
    library.manager().close().await.unwrap();
}

/// The creator's name waits with the library until the open asks for their
/// profile under it, in one write that also deletes the name.
#[tokio::test]
async fn a_creator_s_kept_name_becomes_their_profile_once_storage_is_set_up() {
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = bae_library::test_libraries::storage_library_of(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
        Some("Robin"),
    )
    .await;
    assert!(value_matching(
        &mut library.people().subscribe_profiles(bae_views::sort_language_on),
        |_| true
    )
    .await
    .is_empty());
    connect_storage(library.manager(), library.store(), &storage).await;
    let profile = requested_profile(library.manager(), library.people()).await;
    assert_eq!(profile.name, "Robin");
    let kept = library
        .store()
        .read(|sql| {
            Ok(sql.query_row("SELECT count(*) FROM creator_name", [], |row| row.get::<_, i64>(0))?)
        })
        .await
        .unwrap();
    assert_eq!(kept, 0);
    library.manager().close().await.unwrap();
}

/// The code of the owner's invite `id`, once its sync has granted its access.
async fn granted_code(owner: &coven::CovenHandle, id: coven::InviteId) -> String {
    let mut invites = owner.subscribe(|sql| Ok::<_, coven::CovenError>(coven::Invite::list(sql)?));
    let granted = |invite: &coven::Invite| match bae_library::people::invite_shown(invite) {
        bae_library::people::InviteShown::Code { code, .. } if invite.id == id => Some(code),
        bae_library::people::InviteShown::Code { .. }
        | bae_library::people::InviteShown::Granting => None,
    };
    bae_library::followed::value_matching(&mut invites, |invites| {
        invites.iter().any(|invite| granted(invite).is_some())
    })
    .await
    .iter()
    .find_map(granted)
    .unwrap()
}

/// The inviter an invite made through `membership` names.
async fn inviter_named(
    owner: &coven::CovenHandle,
    membership: &bae_people::Membership,
) -> Result<String, bae_people::membership::InviteError> {
    let id = membership
        .invite_person(
            coven::MemberRole::Member,
            coven::InviteAccess::S3AccessKey {
                access_key_id: "guest-key".into(),
                secret_access_key: coven::SecretText::new("guest-secret".into()),
            },
            coven::InviteLifetime::SevenDays,
        )
        .await?;
    let code = granted_code(owner, id).await;
    let Ok(coven::CodeInfo::Invite(info)) = coven::decode_code_info(&code) else {
        panic!("an invite's code decodes as an invite's");
    };
    Ok(info.inviter)
}

/// An invite names its inviter by the name the person goes by: their
/// profile's, else the name their pending profile request asked for, else the
/// name the library kept from its creation. A person with none of the three
/// gets an error, and coven makes no invite.
#[test]
fn an_invite_names_its_inviter_by_the_name_the_person_goes_by() {
    on_app_runtime(async {
        let clock_of = |sources: &bae_library::test_libraries::TestSources| sources.clock().clone();

        // No name at all.
        let (_dir, owner, _storage, sources) =
            Household::owner(coven::CloudProvider::S3, None).await;
        let store = bae_store::test_devices::store_under(owner.clone(), clock_of(&sources));
        let membership = bae_people::Membership::new(store.clone());
        let people = bae_people::People::new(store);
        assert!(matches!(
            inviter_named(&owner, &membership).await,
            Err(bae_people::membership::InviteError::InviterUnnamed)
        ));
        let open = owner.read(coven::Invite::list).await.unwrap();
        assert!(open.is_empty(), "coven was never called: {open:?}");

        // Only the name the library kept from its creation.
        people.keep_creator_name(&bae_store::EnteredName::parse("Ana").unwrap()).await.unwrap();
        assert_eq!(inviter_named(&owner, &membership).await.unwrap(), "Ana");
        owner.close().await.unwrap();

        // Only a pending profile request.
        let (_dir, owner, _storage, sources) =
            Household::owner(coven::CloudProvider::S3, None).await;
        let store = bae_store::test_devices::store_under(owner.clone(), clock_of(&sources));
        let membership = bae_people::Membership::new(store.clone());
        bae_people::People::new(store).create_profile("Robin").await.unwrap();
        assert_eq!(inviter_named(&owner, &membership).await.unwrap(), "Robin");
        owner.close().await.unwrap();

        // A profile.
        let household = Household::new().await;
        let store = bae_store::test_devices::store_under(
            household.owner.clone(),
            clock_of(&household.sources),
        );
        let membership = bae_people::Membership::new(store);
        assert_eq!(inviter_named(&household.owner, &membership).await.unwrap(), "Robin");
        household.owner.close().await.unwrap();
    });
}

/// An invite made through `Membership` is for a new person and lasts as long
/// as the form chose.
#[test]
fn an_invite_is_for_a_new_person_and_lasts_the_lifetime_chosen() {
    for (lifetime, days) in
        [(coven::InviteLifetime::SevenDays, 7), (coven::InviteLifetime::ThirtyDays, 30)]
    {
        on_app_runtime(async move {
            let household = Household::new().await;
            let store = bae_store::test_devices::store_under(
                household.owner.clone(),
                household.sources.clock().clone(),
            );
            let membership = bae_people::Membership::new(store);
            let before = household.sources.clock().now();

            membership
                .invite_person(
                    coven::MemberRole::Admin,
                    coven::InviteAccess::S3AccessKey {
                        access_key_id: "guest-key".into(),
                        secret_access_key: coven::SecretText::new("guest-secret".into()),
                    },
                    lifetime,
                )
                .await
                .unwrap();

            let open = household.owner.read(coven::Invite::list).await.unwrap();
            let [invite] = open.as_slice() else {
                panic!("one open invite: {open:?}");
            };
            let lasts = std::time::Duration::from_secs(days * 24 * 60 * 60);
            assert_eq!(invite.admits, coven::InviteAdmits::NewPerson);
            assert!(invite.expires_at >= before + lasts, "{:?}", invite.expires_at);
            assert!(
                invite.expires_at <= before + lasts + std::time::Duration::from_secs(60),
                "{:?}",
                invite.expires_at
            );
            household.owner.close().await.unwrap();
        });
    }
}

/// A grant Dropbox refuses for good ends the invite: People lists it as a
/// refused invite, with the address and the refusal, in place of the open
/// invite, until the person dismisses it; dismissing anything else is
/// refused.
#[test]
fn an_invite_dropbox_refuses_to_share_is_listed_as_refused_until_dismissed() {
    on_app_runtime(async {
        let sign_in = DropboxSignIn::start().await;
        let household = Household::on_dropbox(&sign_in).await;
        let store = bae_store::test_devices::store_under(
            household.owner.clone(),
            household.sources.clock().clone(),
        );
        let membership = bae_people::Membership::new(store.clone());
        let mut invites = bae_views::LibraryViews::new(store).subscribe_invites();
        let access = || coven::InviteAccess::ProviderAccount { email: GUEST_ACCOUNT.into() };

        household
            .storage
            .set_faults(coven::Faults {
                grant_failure: Some(coven::StorageFailure::SharingRefused(
                    coven::SharingRefusal::InvalidAddress,
                )),
                ..coven::Faults::none()
            })
            .await;
        membership
            .invite_person(coven::MemberRole::Member, access(), coven::InviteLifetime::SevenDays)
            .await
            .unwrap();
        let listed = bae_library::followed::value_matching(&mut invites, |entries| {
            entries.iter().any(|entry| matches!(entry, bae_views::InviteEntry::Refused(_)))
        })
        .await;
        let [bae_views::InviteEntry::Refused(refused)] = listed.as_slice() else {
            panic!("one refused invite and nothing open: {listed:?}");
        };
        assert_eq!(refused.address, GUEST_ACCOUNT);
        assert_eq!(refused.refusal, coven::SharingRefusal::InvalidAddress);

        // An open invite's operation is no refused invite's.
        household.storage.set_faults(coven::Faults::none()).await;
        membership
            .invite_person(coven::MemberRole::Member, access(), coven::InviteLifetime::SevenDays)
            .await
            .unwrap();
        let both = bae_library::followed::value_matching(&mut invites, |entries| {
            entries.iter().any(|entry| matches!(entry, bae_views::InviteEntry::Open(_)))
        })
        .await;
        let open = both.iter().find_map(bae_views::InviteEntry::open).unwrap();
        assert!(matches!(
            membership.dismiss_refused_invite(open.invite.operation).await,
            Err(bae_people::membership::RefusedInviteDismissError::NotRefusedInvite(_))
        ));

        membership.dismiss_refused_invite(refused.operation).await.unwrap();
        let after = bae_library::followed::value_matching(&mut invites, |entries| {
            !entries.iter().any(|entry| matches!(entry, bae_views::InviteEntry::Refused(_)))
        })
        .await;
        assert_eq!(after.len(), 1, "only the open invite remains: {after:?}");
        assert!(matches!(
            membership.dismiss_refused_invite(refused.operation).await,
            Err(bae_people::membership::RefusedInviteDismissError::NotRefusedInvite(_))
        ));
        household.owner.close().await.unwrap();
    });
}

/// Run a test body on the worker stack the app gives native invite and
/// bootstrap futures.
pub(super) fn on_app_runtime(body: impl std::future::Future<Output = ()> + Send + 'static) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();
    #[expect(
        clippy::disallowed_methods,
        reason = "test code: joined at once, so a panic fails the test"
    )]
    let task = runtime.spawn(body);
    runtime.block_on(task).unwrap();
}

/// An owner's library on shared S3 or Dropbox storage, with Robin's profile.
pub(super) struct Household {
    _dir: tempfile::TempDir,
    pub(super) owner: coven::CovenHandle,
    pub(super) storage: std::sync::Arc<coven::MemoryStorage>,
    pub(super) sources: bae_library::test_libraries::TestSources,
    robin: bae_people::people::PersonProfile,
}

/// The Dropbox account a household on Dropbox invites.
const GUEST_ACCOUNT: &str = "guest@example.com";

impl Household {
    pub(super) async fn new() -> Self {
        Self::on(coven::CloudProvider::S3, None).await
    }

    /// The household on Dropbox, its owner signed in through `sign_in`.
    async fn on_dropbox(sign_in: &DropboxSignIn) -> Self {
        Self::on(coven::CloudProvider::Dropbox, Some(sign_in)).await
    }

    async fn on(provider: coven::CloudProvider, sign_in: Option<&DropboxSignIn>) -> Self {
        let (dir, owner, storage, sources) = Self::owner(provider, sign_in).await;
        let people = bae_people::People::new(bae_store::test_devices::store_under(
            owner.clone(),
            sources.clock().clone(),
        ));
        let robin = made_profile_on(&owner, &people, "Robin").await;
        Self { _dir: dir, owner, storage, sources, robin }
    }

    /// The owner's library on its storage, with no profile yet.
    async fn owner(
        provider: coven::CloudProvider,
        sign_in: Option<&DropboxSignIn>,
    ) -> (
        tempfile::TempDir,
        coven::CovenHandle,
        std::sync::Arc<coven::MemoryStorage>,
        bae_library::test_libraries::TestSources,
    ) {
        use coven::Storage;
        let dir = tempfile::tempdir().unwrap();
        let installation =
            coven::Coven::in_memory(bae_app_dir::AppDir::at(dir.path()).unwrap().coven_app_dir());
        let sources = bae_library::test_libraries::TestSources::system();
        let storage = std::sync::Arc::new(
            coven::MemoryStorage::builder()
                .provider(provider)
                .clock(sources.clock().clone())
                .build()
                .unwrap(),
        );
        let builder = sources.builder(&installation).storage_connector(storage.clone());
        let builder = match sign_in {
            Some(sign_in) => sign_in.configure(builder),
            None => builder,
        };
        // Made without the creator's profile `create_library` asks for, so
        // these tests start from a member with none.
        let owner = bae_store::configure_store(builder, bae_library::SCHEMA)
            .create_store("Household", "Owner Device", &[], |_, _| Ok(()))
            .await
            .unwrap();
        match storage.config().oauth_access() {
            Some(access) => {
                owner.authenticate(access).await.unwrap();
                Box::pin(owner.setup_oauth_storage(storage.config(), &|_| ())).await.unwrap();
            }
            None => {
                Box::pin(owner.setup_s3_storage(
                    storage.config(),
                    "owner-key".into(),
                    coven::SecretText::new("owner-secret".into()),
                    &|_| (),
                ))
                .await
                .unwrap();
            }
        }
        (dir, owner, storage, sources)
    }

    /// An invite giving S3 storage's `key`, or the Dropbox guest's account,
    /// once the owner's sync has granted its access and fixed its code.
    pub(super) async fn invite(&self, key: &str) -> Invited {
        use coven::Storage;
        let access = match self.storage.config().provider() {
            coven::CloudProvider::S3 => coven::InviteAccess::S3AccessKey {
                access_key_id: key.into(),
                secret_access_key: coven::SecretText::new("guest-secret".into()),
            },
            _ => coven::InviteAccess::ProviderAccount { email: GUEST_ACCOUNT.into() },
        };
        let (id, member, _operation) = Box::pin(self.owner.create_invite(
            coven::MemberRole::Member,
            access,
            coven::InviteLifetime::SevenDays,
            "Ana".into(),
        ))
        .await
        .unwrap();
        let mut invites =
            self.owner.subscribe(|sql| Ok::<_, coven::CovenError>(coven::Invite::list(sql)?));
        let granted = |invite: &coven::Invite| match bae_library::people::invite_shown(invite) {
            bae_library::people::InviteShown::Code { code, .. } if invite.id == id => Some(code),
            bae_library::people::InviteShown::Code { .. }
            | bae_library::people::InviteShown::Granting => None,
        };
        let code = bae_library::followed::value_matching(&mut invites, |invites| {
            invites.iter().any(|invite| granted(invite).is_some())
        })
        .await
        .iter()
        .find_map(granted)
        .unwrap();
        Invited { code, member }
    }

    /// A device of its own, `installation`, reaching the household's
    /// storage: with S3's key, or as the Dropbox guest's account.
    pub(super) fn device(&self, installation: &coven::Coven) -> coven::CovenBuilder {
        use coven::Storage;
        let storage = match self.storage.config().provider() {
            coven::CloudProvider::S3 => self.storage.for_device(),
            _ => coven::MemoryStorage::for_recipient(&self.storage, GUEST_ACCOUNT).unwrap(),
        };
        self.sources.builder(installation).storage_connector(std::sync::Arc::new(storage))
    }

    /// A library over the owner's store, in `dir`.
    pub(super) async fn owners_library(
        &self,
        dir: &std::path::Path,
    ) -> bae_library::test_libraries::TestLibrary {
        self.library(self.owner.clone(), dir).await
    }

    /// A library over `handle`, a store of the household on one of its
    /// devices, in `dir`, opened as the app opens one.
    pub(super) async fn library(
        &self,
        handle: coven::CovenHandle,
        dir: &std::path::Path,
    ) -> bae_library::test_libraries::TestLibrary {
        bae_library::test_libraries::test_library_from_handle(
            handle,
            dir,
            bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
            bae_util::http::Http::for_test(),
            &self.sources,
        )
        .await
    }

    /// Join with `code` from a device of its own, named Guest Device.
    pub(super) async fn join(
        &self,
        code: &str,
        installation: &coven::Coven,
        control: &bae_library::JoinControl,
    ) -> Result<Option<bae_library::JoinedLibrary>, bae_library::RestoreFromCodeError> {
        bae_library::join_with_invite(
            self.device(installation),
            code,
            bae_store::EnteredName::parse("Rowan").unwrap(),
            "Guest Device",
            self.sources.clock().clone(),
            |_| {},
            control,
            &NO_DIAGNOSTICS,
        )
        .await
    }

    /// Approve the first request among the owner's open invites.
    pub(super) async fn approve(&self) {
        let mut invites =
            self.owner.subscribe(|sql| Ok::<_, coven::CovenError>(coven::Invite::list(sql)?));
        let deciding = |invite: &coven::Invite| match bae_library::people::invite_shown(invite) {
            bae_library::people::InviteShown::Code {
                progress: bae_library::people::InviteProgress::Deciding(request),
                ..
            } => Some(request),
            bae_library::people::InviteShown::Code { .. }
            | bae_library::people::InviteShown::Granting => None,
        };
        let request = bae_library::followed::value_matching(&mut invites, |invites| {
            invites.iter().any(|invite| deciding(invite).is_some())
        })
        .await
        .iter()
        .find_map(deciding)
        .unwrap();
        self.owner.approve_join_request(&request).await.unwrap();
    }
}

/// The library a join let `guest` into, opened as the app opens one in
/// `dir`, and the profile the joining write made under the invite's name, in
/// the circle the admission made, as the device registered, before any sync
/// of the joined device runs; sync then runs on.
async fn joined_profile(
    guest: coven::CovenHandle,
    household: &Household,
    dir: &std::path::Path,
) -> (bae_library::test_libraries::TestLibrary, bae_people::people::PersonProfile) {
    let library = household.library(guest, dir).await;
    let written = library
        .people()
        .own_person()
        .await
        .unwrap()
        .and_then(|person| person.profile)
        .expect("the joining write made the profile as the device registered");
    let profile = requested_profile(library.manager(), library.people()).await;
    assert_eq!(profile, written);
    (library, profile)
}

/// An invite the owner's sync has granted: the code to join with, and the
/// member id it fixed.
pub(super) struct Invited {
    pub(super) code: String,
    pub(super) member: coven::MemberId,
}

/// Make a circle named `name` on `owner`, syncing until its entry lands.
async fn landed_circle(owner: &coven::CovenHandle, name: &str) -> coven::CircleId {
    let (circle, operation) = Box::pin(owner.circles().create(name)).await.unwrap();
    owner.start_sync().await.unwrap();
    assert!(matches!(
        owner.operation_outcome(operation).await.unwrap(),
        coven::OperationOutcome::Landed(_)
    ));
    owner.stop_sync().await.unwrap();
    owner
        .write(move |sql| Ok::<_, coven::CovenError>(sql.acknowledge_operation(operation)?))
        .await
        .unwrap();
    circle
}

/// A profile whose write failed after its private circle was made retries
/// with that circle: the one circle of the person's name holding only them,
/// in one write, with no circle to wait for. Two circles of that name are no
/// private circle to take.
#[tokio::test]
async fn a_profile_takes_the_private_circle_already_made_for_it() {
    let (_dir, owner, _storage, sources) = Household::owner(coven::CloudProvider::S3, None).await;
    let member =
        owner.members().await.unwrap().into_iter().find(|member| member.is_self).unwrap().id;
    let name = format!("bae-person:{member}");
    let made = landed_circle(&owner, &name).await;
    let people = bae_people::People::new(bae_store::test_devices::store_under(
        owner.clone(),
        sources.clock().clone(),
    ));
    people.create_profile("Robin").await.unwrap();
    let profile = people.profile(member).await.unwrap().unwrap();
    assert_eq!(profile.private_circle, made);
    owner.close().await.unwrap();

    let (_dir, owner, _storage, sources) = Household::owner(coven::CloudProvider::S3, None).await;
    let member =
        owner.members().await.unwrap().into_iter().find(|member| member.is_self).unwrap().id;
    let name = format!("bae-person:{member}");
    for _ in 0..2 {
        landed_circle(&owner, &name).await;
    }
    let people = bae_people::People::new(bae_store::test_devices::store_under(
        owner.clone(),
        sources.clock().clone(),
    ));
    assert!(matches!(
        people.create_profile("Robin").await,
        Err(ProfileError::InvalidPrivateCircle)
    ));
    owner.close().await.unwrap();
}

/// The joining write gives Rowan a profile under the name Robin typed in the
/// invite, in a private circle named for the member the invite fixed; it is
/// Rowan's own row, which Rowan can't use to edit Robin's (§5.4).
#[test]
fn joining_writes_the_invites_name_and_cannot_edit_another_person() {
    on_app_runtime(async {
        let household = Household::new().await;
        let invite = household.invite("guest-key").await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = installation_in(&guest_dir);
        let control = bae_library::JoinControl::new();
        let joining = household.join(&invite.code, &guest, &control);
        let (joined, ()) = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            tokio::join!(joining, household.approve())
        })
        .await
        .unwrap();
        let guest = loaded(joined.unwrap().unwrap());
        let library_dir = tempfile::tempdir().unwrap();
        let (library, profile) =
            joined_profile(guest.clone(), &household, library_dir.path()).await;
        let owner_profile = household.robin.clone();
        assert_eq!(profile.name, "Rowan");
        assert_eq!(profile.id, invite.member);
        let circle_names =
            bae_store::test_devices::store_under(guest.clone(), household.sources.clock().clone())
                .query_texts_for_test(&format!(
                    "SELECT name FROM coven_circles WHERE circle = '{}'",
                    profile.private_circle
                ))
                .await
                .unwrap();
        assert_eq!(circle_names, [format!("bae-person:{}", invite.member)]);
        assert_ne!(profile.id, owner_profile.id);
        assert_ne!(profile.private_circle, owner_profile.private_circle);
        let members = guest.circles().members(profile.private_circle).await.unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].member, profile.id);
        let error = bae_people::People::new(bae_store::test_devices::store_under(
            guest.clone(),
            household.sources.clock().clone(),
        ))
        .rename_profile_of_for_test(owner_profile.id, "Impersonated")
        .await
        .expect_err("only the profile's owner may rename it");
        assert!(
            matches!(error, coven::CovenError::Database(coven::DbError::NotOwner { .. })),
            "{error:?}"
        );
        library.manager().close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}

/// Rowan closes bae while waiting. The next start lists the join with the
/// invite's names, resumes it without the code, and writes Rowan's profile
/// (§5.6).
#[test]
fn a_stopped_join_resumes_and_writes_the_profile() {
    on_app_runtime(async {
        let household = Household::new().await;
        let invite = household.invite("guest-key").await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = installation_in(&guest_dir);
        let control = bae_library::JoinControl::new();
        let (waiting, mut waits) = tokio::sync::watch::channel(false);
        let joining = bae_library::join_with_invite(
            household.device(&guest),
            &invite.code,
            bae_store::EnteredName::parse("Rowan").unwrap(),
            "Guest Device",
            household.sources.clock().clone(),
            |status| {
                if matches!(status, coven::BootstrapStatus::WaitingForApproval { .. }) {
                    waiting.send_replace(true);
                }
            },
            &control,
            &NO_DIAGNOSTICS,
        );
        tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            tokio::select! {
                result = joining => panic!("the join ended while waiting: {:?}", result.err()),
                changed = waits.wait_for(|waiting| *waiting) => drop(changed.unwrap()),
            }
        })
        .await
        .unwrap();
        let builder = household.device(&guest);
        let [join] = &bae_library::unfinished_joins(&builder, &NO_DIAGNOSTICS).await.unwrap()[..]
        else {
            panic!("one join under way");
        };
        assert_eq!(
            (join.store_name.as_str(), join.inviter.as_str(), join.admits),
            ("Household", "Ana", coven::InviteAdmits::NewPerson)
        );
        let resuming = bae_library::resume_join(
            || household.device(&guest),
            join,
            |_| {},
            &control,
            &NO_DIAGNOSTICS,
        );
        let (joined, ()) = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            tokio::join!(resuming, household.approve())
        })
        .await
        .unwrap();
        let guest = loaded(joined.unwrap().unwrap());
        let library_dir = tempfile::tempdir().unwrap();
        let (library, profile) = joined_profile(guest, &household, library_dir.path()).await;
        assert_eq!(profile.name, "Rowan");
        library.manager().close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}

/// Rowan's join waits on the guest device when someone types the owner's
/// restore code there. A restore can't start on the join's installation:
/// coven refuses it as another call's under way, and the join stays listed
/// to go on with (§5.6).
#[test]
fn a_restore_onto_a_device_whose_join_waits_leaves_the_join() {
    on_app_runtime(async {
        let household = Household::new().await;
        let invite = household.invite("guest-key").await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = installation_in(&guest_dir);
        let control = bae_library::JoinControl::new();
        let (waiting, mut waits) = tokio::sync::watch::channel(false);
        let joining = bae_library::join_with_invite(
            household.device(&guest),
            &invite.code,
            bae_store::EnteredName::parse("Rowan").unwrap(),
            "Guest Device",
            household.sources.clock().clone(),
            |status| {
                if matches!(status, coven::BootstrapStatus::WaitingForApproval { .. }) {
                    waiting.send_replace(true);
                }
            },
            &control,
            &NO_DIAGNOSTICS,
        );
        tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            tokio::select! {
                result = joining => panic!("the join ended while waiting: {:?}", result.err()),
                changed = waits.wait_for(|waiting| *waiting) => drop(changed.unwrap()),
            }
        })
        .await
        .unwrap();
        let code = household.owner.restore_code().await.unwrap();
        let (_sender, cancel) = tokio::sync::watch::channel(false);

        let refused = bae_library::restore_from_code(
            household.device(&guest),
            &code,
            Some(coven::S3Credentials {
                access_key_id: "owner-key".into(),
                secret_access_key: coven::SecretText::new("owner-secret".into()),
            }),
            "Guest Device",
            |_| {},
            &cancel,
            &NO_DIAGNOSTICS,
        )
        .await
        .unwrap_err();

        let bae_library::RestoreFromCodeError::Restore(error) = &refused else {
            panic!("coven refuses the restore: {refused:?}");
        };
        assert!(
            matches!(
                **error,
                coven::BootstrapError::Directory(coven::BootstrapDirectoryError::Lock(
                    coven::StoreLockError::BootstrapPending(_),
                ))
            ),
            "{error:?}"
        );
        assert_eq!(
            bae_library::unfinished_joins(&household.device(&guest), &NO_DIAGNOSTICS)
                .await
                .unwrap()
                .len(),
            1
        );
        household.owner.close().await.unwrap();
    });
}

/// A device of its own whose stores live in `dir`, with its own keychain.
pub(super) fn installation_in(dir: &tempfile::TempDir) -> coven::Coven {
    coven::Coven::in_memory(bae_app_dir::AppDir::at(dir.path()).unwrap().coven_app_dir())
}

/// Start Rowan's join on the Dropbox household and stop it while it waits,
/// as closing bae does. Joining signs Rowan in.
async fn stopped_dropbox_join(
    household: &Household,
    sign_in: &DropboxSignIn,
    guest: &coven::Coven,
    control: &bae_library::JoinControl,
) -> (String, coven::InviteInfo) {
    let invite = household.invite("").await;
    let (waiting, mut waits) = tokio::sync::watch::channel(false);
    let joining = bae_library::join_with_invite(
        sign_in.configure(household.device(guest)),
        &invite.code,
        bae_store::EnteredName::parse("Rowan").unwrap(),
        "Guest Device",
        household.sources.clock().clone(),
        |status| {
            if matches!(status, coven::BootstrapStatus::WaitingForApproval { .. }) {
                waiting.send_replace(true);
            }
        },
        control,
        &NO_DIAGNOSTICS,
    );
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        tokio::select! {
            result = joining => panic!("the join ended while waiting: {:?}", result.err()),
            changed = waits.wait_for(|waiting| *waiting) => drop(changed.unwrap()),
        }
    })
    .await
    .unwrap();
    let [join] = &bae_library::unfinished_joins(&household.device(guest), &NO_DIAGNOSTICS)
        .await
        .unwrap()[..]
    else {
        panic!("one join under way");
    };
    (invite.code, join.clone())
}

/// Rowan closes bae while his Dropbox join waits. coven kept his sign-in
/// beside the join, so the next start resumes it without signing him in
/// again (§5.6).
#[test]
fn a_stopped_dropbox_join_resumes_with_the_sign_in_it_kept() {
    on_app_runtime(async {
        let sign_in = DropboxSignIn::start().await;
        let household = Household::on_dropbox(&sign_in).await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = installation_in(&guest_dir);
        let control = bae_library::JoinControl::new();
        let (_, join) = stopped_dropbox_join(&household, &sign_in, &guest, &control).await;
        let presented = sign_in.presented();
        let resuming = bae_library::resume_join(
            || sign_in.configure(household.device(&guest)),
            &join,
            |_| {},
            &control,
            &NO_DIAGNOSTICS,
        );
        let (joined, ()) = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            tokio::join!(resuming, household.approve())
        })
        .await
        .unwrap();
        let guest = loaded(joined.unwrap().unwrap());
        assert_eq!(sign_in.presented(), presented);
        let library_dir = tempfile::tempdir().unwrap();
        let (library, profile) = joined_profile(guest, &household, library_dir.path()).await;
        assert_eq!(profile.name, "Rowan");
        library.manager().close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}

/// Dropbox refuses the sign-in Rowan's waiting join kept, and it can't be
/// refreshed: resuming signs him in once more, then goes on to approval
/// (§5.6).
#[test]
fn a_stopped_join_whose_sign_in_dropbox_refuses_signs_in_again_and_resumes() {
    on_app_runtime(async {
        let sign_in = DropboxSignIn::start().await;
        let household = Household::on_dropbox(&sign_in).await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = installation_in(&guest_dir);
        let control = bae_library::JoinControl::new();
        let (_, join) = stopped_dropbox_join(&household, &sign_in, &guest, &control).await;
        // The owner's sign-in was the first; Rowan's join kept the second.
        household.storage.reject_access_token("access-1").await;
        let presented = sign_in.presented();
        let resuming = bae_library::resume_join(
            || sign_in.configure(household.device(&guest)),
            &join,
            |_| {},
            &control,
            &NO_DIAGNOSTICS,
        );
        let (joined, ()) = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            tokio::join!(resuming, household.approve())
        })
        .await
        .unwrap();
        let guest = loaded(joined.unwrap().unwrap());
        assert_eq!(sign_in.presented(), presented + 1);
        let library_dir = tempfile::tempdir().unwrap();
        let (library, profile) = joined_profile(guest, &household, library_dir.path()).await;
        assert_eq!(profile.name, "Rowan");
        library.manager().close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}

/// The system clock run ahead to the end of the invite `code` holds, as the
/// clock of a device that opens it once it has ended.
fn ended_clock(code: &str) -> coven::ClockRef {
    let coven::CodeInfo::Invite(invite) = coven::decode_code_info(code).unwrap() else {
        panic!("an invite code");
    };
    let ahead = invite.expires_at.duration_since(std::time::SystemTime::now()).unwrap();
    std::sync::Arc::new(AheadClock(ahead))
}

/// The system clock, `self.0` ahead.
struct AheadClock(std::time::Duration);

impl coven::Clock for AheadClock {
    fn now(&self) -> std::time::SystemTime {
        coven::Clock::now(&coven::SystemClock) + self.0
    }
    fn elapsed(&self) -> std::time::Duration {
        coven::Clock::elapsed(&coven::SystemClock)
    }
    fn sleep_until(
        &self,
        at: std::time::SystemTime,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), coven::WallTimerError>> + Send>,
    > {
        coven::Clock::sleep_until(&coven::SystemClock, at - self.0)
    }
}

/// Rowan opens his Dropbox invite only after it has ended. His phone would
/// send no request, so it says the invite ended without asking him to sign in
/// to Dropbox first, and leaves no join behind (§5.6).
#[test]
fn an_ended_dropbox_invite_ends_before_any_sign_in() {
    on_app_runtime(async {
        let sign_in = DropboxSignIn::start().await;
        let household = Household::on_dropbox(&sign_in).await;
        let invite = household.invite("").await;
        let ended = ended_clock(&invite.code);
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = installation_in(&guest_dir);
        let presented = sign_in.presented();
        let joined = bae_library::join_with_invite(
            sign_in.configure(household.device(&guest)).clock(ended.clone()),
            &invite.code,
            bae_store::EnteredName::parse("Rowan").unwrap(),
            "Guest Device",
            ended,
            |_| {},
            &bae_library::JoinControl::new(),
            &NO_DIAGNOSTICS,
        )
        .await;
        assert!(matches!(joined, Ok(None)), "{:?}", joined.err());
        assert_eq!(sign_in.presented(), presented, "no sign-in for an ended invite");
        assert!(bae_library::unfinished_joins(&household.device(&guest), &NO_DIAGNOSTICS)
            .await
            .unwrap()
            .is_empty());
        household.owner.close().await.unwrap();
    });
}

/// Rowan's request was sent before his invite ended, and he enters the code
/// again after: his join is still under way, so it goes on to Robin's
/// approval rather than ending.
#[test]
fn a_join_whose_request_was_sent_goes_on_after_its_invite_ends() {
    on_app_runtime(async {
        let sign_in = DropboxSignIn::start().await;
        let household = Household::on_dropbox(&sign_in).await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = installation_in(&guest_dir);
        let control = bae_library::JoinControl::new();
        let (code, _) = stopped_dropbox_join(&household, &sign_in, &guest, &control).await;
        let ended = ended_clock(&code);
        let joining = bae_library::join_with_invite(
            sign_in.configure(household.device(&guest)).clock(ended.clone()),
            &code,
            bae_store::EnteredName::parse("Rowan").unwrap(),
            "Guest Device",
            ended,
            |_| {},
            &control,
            &NO_DIAGNOSTICS,
        );
        let (joined, ()) = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            tokio::join!(joining, household.approve())
        })
        .await
        .unwrap();
        let guest = loaded(joined.unwrap().expect("the sent request is approved"));
        let library_dir = tempfile::tempdir().unwrap();
        let (library, profile) = joined_profile(guest, &household, library_dir.path()).await;
        assert_eq!(profile.name, "Rowan");
        library.manager().close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}
