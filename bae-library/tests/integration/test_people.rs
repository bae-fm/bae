use super::dropbox_sign_in::DropboxSignIn;
use super::test_joined_library::loaded;
use bae_library::test_libraries::{
    connect_storage, made_profile, made_profile_on, requested_profile, storage_library,
};
use bae_people::people::{ProfileError, Sharing};

mod avatars;

/// The diagnostics these tests' bootstrap calls report to: none.
static NO_DIAGNOSTICS: std::sync::LazyLock<bae_diagnostics::Diagnostics> =
    std::sync::LazyLock::new(bae_diagnostics::Diagnostics::noop);

#[tokio::test]
async fn profile_requires_membership_and_is_idempotent_after_setup() {
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = library.people().clone();
    assert!(matches!(
        library.people().create_profile("Robin").await,
        Err(ProfileError::NotRegistered)
    ));
    assert!(people.profiles().await.unwrap().is_empty());
    connect_storage(library.manager(), library.store(), &storage).await;
    let profile = made_profile(library.manager(), library.people(), "Robin").await;
    assert_eq!(profile.history_sharing, Sharing::Private);
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
    library.people().set_history_sharing(Sharing::Household).await.unwrap();
    let profile = people.profile(profile.id).await.unwrap().unwrap();
    assert_eq!(profile.name, "New Name");
    assert_eq!(profile.history_sharing, Sharing::Household);
    library.manager().close().await.unwrap();
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
    sources: bae_library::test_libraries::TestSources,
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
        let owner = bae_library::create_library(
            builder,
            bae_store::EnteredName::parse("Household").unwrap(),
            &NO_DIAGNOSTICS,
        )
        .await
        .unwrap();
        match storage.config().oauth_access() {
            Some(access) => {
                owner.authenticate(access).await.unwrap();
                Box::pin(owner.setup_oauth_storage(storage.config(), "Owner Device", &|_| ()))
                    .await
                    .unwrap();
            }
            None => {
                Box::pin(owner.setup_s3_storage(
                    storage.config(),
                    "Owner Device",
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
    pub(super) async fn invite(&self, target: coven::InviteTarget, key: &str) -> Invited {
        use coven::Storage;
        let access = match self.storage.config().provider() {
            coven::CloudProvider::S3 => coven::InviteAccess::S3AccessKey {
                access_key_id: key.into(),
                secret_access_key: coven::SecretText::new("guest-secret".into()),
            },
            _ => coven::InviteAccess::ProviderAccount { email: GUEST_ACCOUNT.into() },
        };
        let (id, member, _operation) =
            Box::pin(self.owner.create_invite(target, access, coven::InviteLifetime::Day))
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
/// `dir`, and the profile its open asked for under the invite's name,
/// written once the joined device has synced the person's private circle.
async fn joined_profile(
    guest: coven::CovenHandle,
    household: &Household,
    dir: &std::path::Path,
) -> (bae_library::test_libraries::TestLibrary, bae_people::people::PersonProfile) {
    let library = household.library(guest, dir).await;
    let profile = requested_profile(library.manager(), library.people()).await;
    (library, profile)
}

/// An invite the owner's sync has granted: the code to join with, and the
/// member id it fixed.
pub(super) struct Invited {
    pub(super) code: String,
    pub(super) member: coven::MemberId,
}

pub(super) fn new_member(name: &str) -> coven::InviteTarget {
    coven::InviteTarget::NewMember { role: coven::MemberRole::Member, name: name.into() }
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

/// The joined library's open asks for Rowan's profile under the name Robin
/// typed in the invite; it is Rowan's own row, which Rowan can't use to edit
/// Robin's (§5.4).
#[test]
fn joining_writes_the_invites_name_and_cannot_edit_another_person() {
    on_app_runtime(async {
        let household = Household::new().await;
        let invite = household.invite(new_member("Rowan"), "guest-key").await;
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

/// Rowan loses every device and is invited again under the same member id.
/// The invite names nobody, so coven keeps no name for the library's open to
/// ask for a profile under: Rowan keeps the row they had (§5.4).
#[test]
fn a_replacement_invite_writes_no_profile() {
    on_app_runtime(async {
        let household = Household::new().await;
        let invite = household.invite(new_member("Rowan"), "guest-key").await;
        let first_dir = tempfile::tempdir().unwrap();
        let first = installation_in(&first_dir);
        let control = bae_library::JoinControl::new();
        let joining = household.join(&invite.code, &first, &control);
        let (joined, ()) = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            tokio::join!(joining, household.approve())
        })
        .await
        .unwrap();
        let lost = loaded(joined.unwrap().unwrap());
        let lost_dir = tempfile::tempdir().unwrap();
        let (lost_library, rowan) = joined_profile(lost.clone(), &household, lost_dir.path()).await;
        // Rowan's profile reaches storage before the device is lost.
        assert!(matches!(
            lost.retry_now().await.unwrap().await,
            coven::RetryOutcome::Synced { .. }
        ));
        lost_library.manager().close().await.unwrap();
        let again = household
            .invite(
                coven::InviteTarget::ReplaceMemberKeys { member: invite.member },
                "replacement-key",
            )
            .await;
        let second_dir = tempfile::tempdir().unwrap();
        let second = installation_in(&second_dir);
        let joining = bae_library::join_with_invite(
            household.device(&second),
            &again.code,
            "Replacement Device",
            household.sources.clock().clone(),
            |_| {},
            &control,
            &NO_DIAGNOSTICS,
        );
        let (joined, ()) = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            tokio::join!(joining, household.approve())
        })
        .await
        .unwrap();
        let replacement = loaded(joined.unwrap().unwrap());
        assert_eq!(replacement.joined_invitee().await.unwrap(), None);
        let people = bae_people::People::new(bae_store::test_devices::store_under(
            replacement.clone(),
            household.sources.clock().clone(),
        ));
        assert_eq!(people.profile(rowan.id).await.unwrap(), Some(rowan.clone()));
        // The owner's membership lists each member's devices that can sync
        // apart from those that lost access, as coven's own member list has
        // them.
        let owner_store = bae_store::test_devices::store_under(
            household.owner.clone(),
            household.sources.clock().clone(),
        );
        let owner = bae_people::Membership::new(owner_store.clone());
        let owner_people = bae_people::People::new(owner_store);
        let membership = owner.members().await.unwrap();
        assert!(membership.self_is_admin);
        let listed: Vec<_> = household
            .owner
            .members()
            .await
            .unwrap()
            .into_iter()
            .map(|member| {
                let (active, former): (Vec<_>, Vec<_>) = member
                    .devices
                    .into_iter()
                    .partition(|device| device.state == coven::DeviceState::Active);
                (
                    member.id,
                    member.keys,
                    member.role,
                    member.access,
                    member.is_self,
                    active.into_iter().map(|device| (device.id, device.name)).collect::<Vec<_>>(),
                    former.into_iter().map(|device| (device.id, device.name)).collect::<Vec<_>>(),
                )
            })
            .collect();
        let read: Vec<_> = membership
            .members
            .iter()
            .map(|member| {
                (
                    member.id,
                    member.keys,
                    member.role,
                    member.access.clone(),
                    member.is_self,
                    member
                        .active_devices
                        .iter()
                        .map(|device| (device.id, device.name.clone()))
                        .collect::<Vec<_>>(),
                    member
                        .former_devices
                        .iter()
                        .map(|device| (device.id, device.name.clone()))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(read, listed);
        // Everyone in the library, as the membership lists them, each with
        // their profile; and the person using this device among them.
        let people = owner_people.people().await.unwrap();
        assert_eq!(
            people.iter().map(|person| person.membership.clone()).collect::<Vec<_>>(),
            membership.members
        );
        for person in &people {
            assert_eq!(person.profile, owner_people.profile(person.membership.id).await.unwrap());
        }
        assert!(people
            .iter()
            .any(|person| person.membership.id == rowan.id && person.profile.is_some()));
        let own = owner_people.own_person().await.unwrap().expect("the owner is in");
        assert!(own.membership.is_self);
        assert_eq!(own.profile.as_ref(), Some(&household.robin));
        replacement.close().await.unwrap();
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
        let invite = household.invite(new_member("Rowan"), "guest-key").await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = installation_in(&guest_dir);
        let control = bae_library::JoinControl::new();
        let (waiting, mut waits) = tokio::sync::watch::channel(false);
        let joining = bae_library::join_with_invite(
            household.device(&guest),
            &invite.code,
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
            (join.store_name.as_str(), join.invitee.as_deref()),
            ("Household", Some("Rowan"))
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
        let invite = household.invite(new_member("Rowan"), "guest-key").await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = installation_in(&guest_dir);
        let control = bae_library::JoinControl::new();
        let (waiting, mut waits) = tokio::sync::watch::channel(false);
        let joining = bae_library::join_with_invite(
            household.device(&guest),
            &invite.code,
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
    let invite = household.invite(new_member("Rowan"), "").await;
    let (waiting, mut waits) = tokio::sync::watch::channel(false);
    let joining = bae_library::join_with_invite(
        sign_in.configure(household.device(guest)),
        &invite.code,
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
        let invite = household.invite(new_member("Rowan"), "").await;
        let ended = ended_clock(&invite.code);
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = installation_in(&guest_dir);
        let presented = sign_in.presented();
        let joined = bae_library::join_with_invite(
            sign_in.configure(household.device(&guest)).clock(ended.clone()),
            &invite.code,
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
