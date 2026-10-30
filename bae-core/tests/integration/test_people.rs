use bae_core::people::{ProfileError, Sharing};
use bae_test_support::{connect_storage, storage_library};

#[tokio::test]
async fn profile_requires_membership_and_is_idempotent_after_setup() {
    let directory = tempfile::tempdir().unwrap();
    let (manager, database, storage) =
        storage_library(directory.path(), bae_core::providers::Providers::offline()).await;
    assert!(matches!(
        manager.create_profile("Robin").await,
        Err(ProfileError::NotRegistered)
    ));
    assert!(database.profiles().await.unwrap().is_empty());
    connect_storage(&manager, &storage).await;
    let profile = manager.create_profile("Robin").await.unwrap();
    assert_eq!(profile.history_sharing, Sharing::Private);
    assert_eq!(manager.create_profile("Robin").await.unwrap(), profile);
    assert!(matches!(
        manager.create_profile("Another Name").await,
        Err(ProfileError::AlreadyExists)
    ));
    let people = manager.people().await.unwrap();
    assert_eq!(people.len(), 1);
    assert_eq!(people[0].profile.as_ref(), Some(&profile));
    assert!(people[0].membership.is_self);
    manager.rename_profile("New Name").await.unwrap();
    manager
        .set_history_sharing(Sharing::Household)
        .await
        .unwrap();
    let profile = database.profile(profile.id).await.unwrap().unwrap();
    assert_eq!(profile.name, "New Name");
    assert_eq!(profile.history_sharing, Sharing::Household);
    manager.close().await.unwrap();
}

/// A PNG of `width` by `height` pixels in one color.
fn solid_png(width: u32, height: u32, color: [u8; 3]) -> Vec<u8> {
    png(image::RgbImage::from_pixel(
        width,
        height,
        image::Rgb(color),
    ))
}

fn png(image: image::RgbImage) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

#[tokio::test]
async fn a_profile_names_each_new_avatar_at_once() {
    let directory = tempfile::tempdir().unwrap();
    let (manager, database, storage) =
        storage_library(directory.path(), bae_core::providers::Providers::offline()).await;
    connect_storage(&manager, &storage).await;
    let profile = manager.create_profile("Rowan").await.unwrap();
    assert_eq!(manager.avatar(profile.id).await.unwrap(), None);
    let first_source = solid_png(16, 16, [120, 40, 200]);
    let second_source = solid_png(16, 16, [20, 160, 90]);
    let first_jpeg = bae_util::stored_image::stored_jpeg(&first_source).unwrap();
    let second_jpeg = bae_util::stored_image::stored_jpeg(&second_source).unwrap();
    manager.set_avatar(Some(first_source)).await.unwrap();
    let first = database.profile(profile.id).await.unwrap().unwrap();
    assert_eq!(
        manager.avatar(profile.id).await.unwrap().unwrap(),
        first_jpeg
    );
    assert_eq!(
        database
            .queued_upload_count_in_table_for_test("people")
            .await
            .unwrap(),
        1,
        "the avatar waits to upload"
    );
    manager.set_avatar(Some(second_source)).await.unwrap();
    let second = database.profile(profile.id).await.unwrap().unwrap();
    // The profile row references each new avatar's coven file from the write
    // that sets it; a new avatar is a new file.
    assert!(first.avatar_id.is_some());
    assert!(second.avatar_id.is_some());
    assert_ne!(first.avatar_id, second.avatar_id);
    assert_eq!(
        manager.avatar(profile.id).await.unwrap().unwrap(),
        second_jpeg
    );
    manager.drain_uploads_for_test().await.unwrap();
    let stored = database.profile(profile.id).await.unwrap().unwrap();
    assert_eq!(
        stored.avatar_id, second.avatar_id,
        "the upload changes no row"
    );
    assert_eq!(
        manager.avatar(profile.id).await.unwrap().unwrap(),
        second_jpeg
    );
    manager.set_avatar(None).await.unwrap();
    assert_eq!(manager.avatar(profile.id).await.unwrap(), None);
    assert_eq!(
        database
            .profile(profile.id)
            .await
            .unwrap()
            .unwrap()
            .avatar_id,
        None
    );
    manager.close().await.unwrap();
}

/// A camera-sized photo is stored as a cover is: a JPEG downscaled to fit the
/// stored edge, its aspect kept. Its pixels vary everywhere, so neither the
/// PNG source nor the JPEG compresses it away.
#[tokio::test]
async fn an_oversized_avatar_is_stored_as_a_downscaled_jpeg() {
    let directory = tempfile::tempdir().unwrap();
    let (manager, _database, storage) =
        storage_library(directory.path(), bae_core::providers::Providers::offline()).await;
    connect_storage(&manager, &storage).await;
    let profile = manager.create_profile("Rowan").await.unwrap();
    let edge = bae_util::stored_image::STORED_MAX_EDGE;
    let photo = png(image::RgbImage::from_fn(edge * 5, edge * 10 / 3, |x, y| {
        let mixed = x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(2_246_822_519);
        let [r, g, b, _] = mixed.to_le_bytes();
        image::Rgb([r, g, b])
    }));

    manager.set_avatar(Some(photo)).await.unwrap();

    let stored = manager.avatar(profile.id).await.unwrap().unwrap();
    assert_eq!(
        image::guess_format(&stored).unwrap(),
        image::ImageFormat::Jpeg
    );
    let decoded = image::load_from_memory(&stored).unwrap();
    assert_eq!(
        (decoded.width(), decoded.height()),
        (edge, edge * 2 / 3),
        "downscaled to fit the stored edge, aspect kept"
    );
    manager.close().await.unwrap();
}

/// Bytes that are no image make no avatar: the setting is refused and the
/// profile keeps none.
#[tokio::test]
async fn an_undecodable_avatar_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let (manager, _database, storage) =
        storage_library(directory.path(), bae_core::providers::Providers::offline()).await;
    connect_storage(&manager, &storage).await;
    let profile = manager.create_profile("Rowan").await.unwrap();

    let error = manager
        .set_avatar(Some(b"not an image".to_vec()))
        .await
        .expect_err("bytes that are no image make no avatar");

    assert!(matches!(error, ProfileError::AvatarImage { .. }), "{error}");
    assert_eq!(manager.avatar(profile.id).await.unwrap(), None);
    manager.close().await.unwrap();
}

/// Run a test body on the worker stack the app gives native invite and
/// bootstrap futures.
fn on_app_runtime(body: impl std::future::Future<Output = ()> + Send + 'static) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(runtime.spawn(body)).unwrap();
}

/// An owner's library on shared S3 or Dropbox storage, with Robin's profile.
struct Household {
    _dir: tempfile::TempDir,
    owner: coven::CovenHandle,
    storage: std::sync::Arc<coven::MemoryStorage>,
    clock: coven::ClockRef,
    robin: bae_core::people::PersonProfile,
}

/// The Dropbox account a household on Dropbox invites.
const GUEST_ACCOUNT: &str = "guest@example.com";

impl Household {
    async fn new() -> Self {
        Self::on(coven::CloudProvider::S3, None).await
    }

    /// The household on Dropbox, its owner signed in through `sign_in`.
    async fn on_dropbox(sign_in: &DropboxSignIn) -> Self {
        Self::on(coven::CloudProvider::Dropbox, Some(sign_in)).await
    }

    async fn on(provider: coven::CloudProvider, sign_in: Option<&DropboxSignIn>) -> Self {
        let (dir, owner, storage, clock) = Self::owner(provider, sign_in).await;
        let robin = bae_core::db::Database::from_handle(owner.clone(), clock.clone())
            .create_profile("Robin")
            .await
            .unwrap();
        Self {
            _dir: dir,
            owner,
            storage,
            clock,
            robin,
        }
    }

    /// The owner's library on its storage, with no profile yet.
    async fn owner(
        provider: coven::CloudProvider,
        sign_in: Option<&DropboxSignIn>,
    ) -> (
        tempfile::TempDir,
        coven::CovenHandle,
        std::sync::Arc<coven::MemoryStorage>,
        coven::ClockRef,
    ) {
        use coven::Storage;
        let dir = tempfile::tempdir().unwrap();
        let installation = coven::TestCoven::new();
        let clock: coven::ClockRef = std::sync::Arc::new(coven::SystemClock);
        let storage = std::sync::Arc::new(
            coven::MemoryStorage::builder()
                .provider(provider)
                .clock(clock.clone())
                .build()
                .unwrap(),
        );
        let builder = installation
            .builder(
                bae_core::config::AppDir::at(dir.path())
                    .unwrap()
                    .store_layout(),
            )
            .storage_connector(storage.clone());
        let builder = match sign_in {
            Some(sign_in) => sign_in.configure(builder),
            None => builder,
        };
        let owner = bae_core::library::create_library(
            builder,
            bae_core::library_name::LibraryName::parse("Household").unwrap(),
        )
        .await
        .unwrap();
        match storage.config().oauth_access() {
            Some(access) => {
                owner.authenticate(access).await.unwrap();
                Box::pin(owner.setup_oauth_storage(storage.config(), "Owner Device"))
                    .await
                    .unwrap();
            }
            None => {
                Box::pin(owner.setup_s3_storage(
                    storage.config(),
                    "Owner Device",
                    "owner-key".into(),
                    coven::SecretText::new("owner-secret".into()),
                ))
                .await
                .unwrap();
            }
        }
        (dir, owner, storage, clock)
    }

    /// An invite giving S3 storage's `key`, or the Dropbox guest's account.
    async fn invite(&self, target: coven::InviteTarget, key: &str) -> coven::Invite {
        use coven::Storage;
        let access = match self.storage.config().provider() {
            coven::CloudProvider::S3 => coven::InviteAccess::S3AccessKey {
                access_key_id: key.into(),
                secret_access_key: coven::SecretText::new("guest-secret".into()),
            },
            _ => coven::InviteAccess::ProviderAccount {
                email: GUEST_ACCOUNT.into(),
            },
        };
        Box::pin(self.owner.create_invite(
            target,
            access,
            coven::InviteDelivery::Sent {
                lifetime: coven::InviteLifetime::Day,
            },
        ))
        .await
        .unwrap()
    }

    /// A device of its own, with its own keychain, reaching the household's
    /// storage: with S3's key, or as the Dropbox guest's account.
    fn device(
        &self,
        installation: &coven::TestCoven,
        dir: &tempfile::TempDir,
    ) -> coven::CovenBuilder {
        use coven::Storage;
        let storage = match self.storage.config().provider() {
            coven::CloudProvider::S3 => self.storage.for_device(),
            _ => coven::MemoryStorage::for_recipient(&self.storage, GUEST_ACCOUNT).unwrap(),
        };
        installation
            .builder(
                bae_core::config::AppDir::at(dir.path())
                    .unwrap()
                    .store_layout(),
            )
            .storage_connector(std::sync::Arc::new(storage))
    }

    /// Approve the first request among the owner's open invites.
    async fn approve(&self) {
        let mut invites = self.owner.subscribe_invites();
        let request = invites
            .wait_for(|invites| invites.iter().any(|invite| invite.request.is_some()))
            .await
            .unwrap()
            .iter()
            .find_map(|invite| invite.request.clone())
            .unwrap();
        self.owner.approve_join_request(&request).await.unwrap();
    }
}

fn new_member(name: &str) -> coven::InviteTarget {
    coven::InviteTarget::NewMember {
        role: coven::MemberRole::Member,
        name: name.into(),
    }
}

/// A profile whose write failed after its private circle was made retries
/// with that circle: the one circle of the person's name holding only them.
/// Two circles of that name are no private circle to take.
#[tokio::test]
async fn a_profile_takes_the_private_circle_already_made_for_it() {
    let (_dir, owner, _storage, clock) = Household::owner(coven::CloudProvider::S3, None).await;
    let member = owner
        .members()
        .await
        .unwrap()
        .into_iter()
        .find(|member| member.is_self)
        .unwrap()
        .id;
    let name = format!("bae-person:{member}");
    let made = Box::pin(owner.circles().create(&name)).await.unwrap();
    let database = bae_core::db::Database::from_handle(owner.clone(), clock.clone());
    let profile = database.create_profile("Robin").await.unwrap();
    assert_eq!(profile.private_circle, made);
    owner.close().await.unwrap();

    let (_dir, owner, _storage, clock) = Household::owner(coven::CloudProvider::S3, None).await;
    let member = owner
        .members()
        .await
        .unwrap()
        .into_iter()
        .find(|member| member.is_self)
        .unwrap()
        .id;
    let name = format!("bae-person:{member}");
    for _ in 0..2 {
        Box::pin(owner.circles().create(&name)).await.unwrap();
    }
    let database = bae_core::db::Database::from_handle(owner.clone(), clock);
    assert!(matches!(
        database.create_profile("Robin").await,
        Err(ProfileError::InvalidPrivateCircle)
    ));
    owner.close().await.unwrap();
}

/// Rowan's profile takes the name Robin typed in the invite; it is Rowan's
/// own row, which Rowan can't use to edit Robin's (§5.4).
#[test]
fn joining_writes_the_invites_name_and_cannot_edit_another_person() {
    on_app_runtime(async {
        use std::time::Duration;
        let household = Household::new().await;
        let invite = household.invite(new_member("Rowan"), "guest-key").await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = coven::TestCoven::new();
        let control = bae_core::library::JoinControl::new();
        let device_app_dir = bae_core::config::AppDir::at(guest_dir.path()).unwrap();
        let joining = bae_core::library::join_with_invite(
            household.device(&guest, &guest_dir),
            &device_app_dir,
            &invite.code,
            "Guest Device",
            household.clock.clone(),
            |_| {},
            &control,
        );
        let (joined, ()) = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(joining, household.approve())
        })
        .await
        .unwrap();
        let (guest, profile) = joined.unwrap().unwrap();
        let profile = profile.expect("a new person's profile").unwrap();
        let owner_profile = household.robin.clone();
        assert_eq!(profile.name, "Rowan");
        assert_ne!(profile.id, owner_profile.id);
        assert_ne!(profile.private_circle, owner_profile.private_circle);
        let members = guest
            .circles()
            .members(profile.private_circle)
            .await
            .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].member, profile.id);
        let error = guest
            .write(move |sql| {
                sql.execute(
                    "UPDATE people SET name = 'Impersonated' WHERE id = ?",
                    [owner_profile.id.to_string()],
                )?;
                Ok(())
            })
            .await
            .expect_err("only the profile's owner may rename it");
        assert!(
            matches!(
                error,
                coven::CovenError::Database(coven::DbError::NotOwner { .. })
            ),
            "{error:?}"
        );
        guest.close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}

/// Rowan loses every device and is invited again under the same member id.
/// The invite names nobody, so joining writes no profile: Rowan keeps the row
/// they had (§5.4).
#[test]
fn a_replacement_invite_writes_no_profile() {
    on_app_runtime(async {
        use std::time::Duration;
        let household = Household::new().await;
        let invite = household.invite(new_member("Rowan"), "guest-key").await;
        let first_dir = tempfile::tempdir().unwrap();
        let first = coven::TestCoven::new();
        let control = bae_core::library::JoinControl::new();
        let device_app_dir = bae_core::config::AppDir::at(first_dir.path()).unwrap();
        let joining = bae_core::library::join_with_invite(
            household.device(&first, &first_dir),
            &device_app_dir,
            &invite.code,
            "Guest Device",
            household.clock.clone(),
            |_| {},
            &control,
        );
        let (joined, ()) = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(joining, household.approve())
        })
        .await
        .unwrap();
        let (lost, profile) = joined.unwrap().unwrap();
        let rowan = profile.unwrap().unwrap();
        // Rowan's profile reaches storage before the device is lost.
        lost.start_sync().await.unwrap();
        assert!(matches!(
            lost.retry_now().await,
            coven::RetryOutcome::Synced { .. }
        ));
        lost.close().await.unwrap();
        let again = household
            .invite(
                coven::InviteTarget::ReplaceMemberKeys {
                    member: invite.member,
                },
                "replacement-key",
            )
            .await;
        let second_dir = tempfile::tempdir().unwrap();
        let second = coven::TestCoven::new();
        let device_app_dir = bae_core::config::AppDir::at(second_dir.path()).unwrap();
        let joining = bae_core::library::join_with_invite(
            household.device(&second, &second_dir),
            &device_app_dir,
            &again.code,
            "Replacement Device",
            household.clock.clone(),
            |_| {},
            &control,
        );
        let (joined, ()) = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(joining, household.approve())
        })
        .await
        .unwrap();
        let (replacement, profile) = joined.unwrap().unwrap();
        assert!(profile.is_none());
        let database =
            bae_core::db::Database::from_handle(replacement.clone(), household.clock.clone());
        assert_eq!(
            database.profile(rowan.id).await.unwrap(),
            Some(rowan.clone())
        );
        // The owner's membership lists each member's devices that can sync
        // apart from those that lost access, as coven's own member list has
        // them.
        let owner =
            bae_core::db::Database::from_handle(household.owner.clone(), household.clock.clone());
        let membership = owner.membership().await.unwrap();
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
                    active
                        .into_iter()
                        .map(|device| (device.id, device.name))
                        .collect::<Vec<_>>(),
                    former
                        .into_iter()
                        .map(|device| (device.id, device.name))
                        .collect::<Vec<_>>(),
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
        let people = owner.people().await.unwrap();
        assert_eq!(
            people
                .iter()
                .map(|person| person.membership.clone())
                .collect::<Vec<_>>(),
            membership.members
        );
        for person in &people {
            assert_eq!(
                person.profile,
                owner.profile(person.membership.id).await.unwrap()
            );
        }
        assert!(people
            .iter()
            .any(|person| person.membership.id == rowan.id && person.profile.is_some()));
        let own = owner.own_person().await.unwrap().expect("the owner is in");
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
        use std::time::Duration;
        let household = Household::new().await;
        let invite = household.invite(new_member("Rowan"), "guest-key").await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = coven::TestCoven::new();
        let control = bae_core::library::JoinControl::new();
        let (waiting, mut waits) = tokio::sync::watch::channel(false);
        let device_app_dir = bae_core::config::AppDir::at(guest_dir.path()).unwrap();
        let joining = bae_core::library::join_with_invite(
            household.device(&guest, &guest_dir),
            &device_app_dir,
            &invite.code,
            "Guest Device",
            household.clock.clone(),
            |status| {
                if matches!(status, coven::BootstrapStatus::WaitingForApproval { .. }) {
                    waiting.send_replace(true);
                }
            },
            &control,
        );
        tokio::time::timeout(Duration::from_secs(30), async {
            tokio::select! {
                result = joining => panic!("the join ended while waiting: {:?}", result.err()),
                changed = waits.wait_for(|waiting| *waiting) => drop(changed.unwrap()),
            }
        })
        .await
        .unwrap();
        let builder = household.device(&guest, &guest_dir);
        let [join] = &bae_core::library::unfinished_joins(&builder).await.unwrap()[..] else {
            panic!("one join under way");
        };
        assert_eq!(
            (join.store_name.as_str(), join.invitee.as_deref()),
            ("Household", Some("Rowan"))
        );
        let device_app_dir = bae_core::config::AppDir::at(guest_dir.path()).unwrap();
        let resuming = bae_core::library::resume_join(
            || household.device(&guest, &guest_dir),
            &device_app_dir,
            join,
            household.clock.clone(),
            |_| {},
            &control,
        );
        let (joined, ()) = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(resuming, household.approve())
        })
        .await
        .unwrap();
        let (guest, profile) = joined.unwrap().unwrap();
        assert_eq!(profile.unwrap().unwrap().name, "Rowan");
        guest.close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}

/// Signs bae in to a fake Dropbox. The presenter answers every authorization
/// with a code, and the token endpoint grants `access-N` for the Nth code. It
/// refuses every refresh, so a token Dropbox rejects needs a new sign-in.
struct DropboxSignIn {
    endpoint: String,
    presented: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
}

impl DropboxSignIn {
    async fn start() -> Self {
        use axum::{http::StatusCode, Form, Json};
        let granted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let router = axum::Router::new().fallback(
            move |Form(form): Form<std::collections::HashMap<String, String>>| {
                let granted = granted.clone();
                async move {
                    if form.get("grant_type").map(String::as_str) == Some("refresh_token") {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(serde_json::json!({ "error": "invalid_grant" })),
                        );
                    }
                    let n = granted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "access_token": format!("access-{n}"),
                            "refresh_token": format!("refresh-{n}"),
                            "token_type": "Bearer",
                            "expires_in": 3600,
                        })),
                    )
                }
            },
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self {
            endpoint,
            presented: std::sync::Arc::default(),
            server,
        }
    }

    fn configure(&self, builder: coven::CovenBuilder) -> coven::CovenBuilder {
        builder
            .oauth_clients(
                coven::OAuthClients::new(
                    Some(coven::DropboxClients::FullDropbox {
                        app_key: "dropbox-key".into(),
                    }),
                    None,
                    std::sync::Arc::new(coven::SystemClock),
                )
                .with_token_endpoint(self.endpoint.clone()),
            )
            .oauth_presenter(std::sync::Arc::new(CodePresenter {
                presented: self.presented.clone(),
            }))
    }

    /// How many sign-ins bae has presented.
    fn presented(&self) -> usize {
        self.presented.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Drop for DropboxSignIn {
    fn drop(&mut self) {
        self.server.abort();
    }
}

/// Answers each authorization with a code for its own state, as the person
/// approving it in Dropbox's sheet would.
struct CodePresenter {
    presented: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[async_trait::async_trait]
impl coven::OAuthPresenter for CodePresenter {
    fn redirect_uri(&self, _: coven::CloudProvider) -> &str {
        "bae-test:/oauth"
    }

    async fn present(
        &self,
        authorization_url: &str,
    ) -> Result<coven::SecretText, coven::OAuthError> {
        self.presented
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let url = url::Url::parse(authorization_url).unwrap();
        let (_, state) = url.query_pairs().find(|(name, _)| name == "state").unwrap();
        Ok(coven::SecretText::new(format!(
            "bae-test:/oauth?code=approved&state={state}"
        )))
    }
}

/// Start Rowan's join on the Dropbox household and stop it while it waits,
/// as closing bae does. Joining signs Rowan in.
async fn stopped_dropbox_join(
    household: &Household,
    sign_in: &DropboxSignIn,
    guest: &coven::TestCoven,
    guest_dir: &tempfile::TempDir,
    control: &bae_core::library::JoinControl,
) -> coven::InviteInfo {
    use std::time::Duration;
    let invite = household.invite(new_member("Rowan"), "").await;
    let (waiting, mut waits) = tokio::sync::watch::channel(false);
    let device_app_dir = bae_core::config::AppDir::at(guest_dir.path()).unwrap();
    let joining = bae_core::library::join_with_invite(
        sign_in.configure(household.device(guest, guest_dir)),
        &device_app_dir,
        &invite.code,
        "Guest Device",
        household.clock.clone(),
        |status| {
            if matches!(status, coven::BootstrapStatus::WaitingForApproval { .. }) {
                waiting.send_replace(true);
            }
        },
        control,
    );
    tokio::time::timeout(Duration::from_secs(30), async {
        tokio::select! {
            result = joining => panic!("the join ended while waiting: {:?}", result.err()),
            changed = waits.wait_for(|waiting| *waiting) => drop(changed.unwrap()),
        }
    })
    .await
    .unwrap();
    let [join] = &bae_core::library::unfinished_joins(&household.device(guest, guest_dir))
        .await
        .unwrap()[..]
    else {
        panic!("one join under way");
    };
    join.clone()
}

/// Rowan closes bae while his Dropbox join waits. coven kept his sign-in
/// beside the join, so the next start resumes it without signing him in
/// again (§5.6).
#[test]
fn a_stopped_dropbox_join_resumes_with_the_sign_in_it_kept() {
    on_app_runtime(async {
        use std::time::Duration;
        let sign_in = DropboxSignIn::start().await;
        let household = Household::on_dropbox(&sign_in).await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = coven::TestCoven::new();
        let control = bae_core::library::JoinControl::new();
        let join = stopped_dropbox_join(&household, &sign_in, &guest, &guest_dir, &control).await;
        let presented = sign_in.presented();
        let device_app_dir = bae_core::config::AppDir::at(guest_dir.path()).unwrap();
        let resuming = bae_core::library::resume_join(
            || sign_in.configure(household.device(&guest, &guest_dir)),
            &device_app_dir,
            &join,
            household.clock.clone(),
            |_| {},
            &control,
        );
        let (joined, ()) = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(resuming, household.approve())
        })
        .await
        .unwrap();
        let (guest, profile) = joined.unwrap().unwrap();
        assert_eq!(sign_in.presented(), presented);
        assert_eq!(profile.unwrap().unwrap().name, "Rowan");
        guest.close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}

/// Dropbox refuses the sign-in Rowan's waiting join kept, and it can't be
/// refreshed: resuming signs him in once more, then goes on to approval
/// (§5.6).
#[test]
fn a_stopped_join_whose_sign_in_dropbox_refuses_signs_in_again_and_resumes() {
    on_app_runtime(async {
        use std::time::Duration;
        let sign_in = DropboxSignIn::start().await;
        let household = Household::on_dropbox(&sign_in).await;
        let guest_dir = tempfile::tempdir().unwrap();
        let guest = coven::TestCoven::new();
        let control = bae_core::library::JoinControl::new();
        let join = stopped_dropbox_join(&household, &sign_in, &guest, &guest_dir, &control).await;
        // The owner's sign-in was the first; Rowan's join kept the second.
        household.storage.reject_access_token("access-1").await;
        let presented = sign_in.presented();
        let device_app_dir = bae_core::config::AppDir::at(guest_dir.path()).unwrap();
        let resuming = bae_core::library::resume_join(
            || sign_in.configure(household.device(&guest, &guest_dir)),
            &device_app_dir,
            &join,
            household.clock.clone(),
            |_| {},
            &control,
        );
        let (joined, ()) = tokio::time::timeout(Duration::from_secs(30), async {
            tokio::join!(resuming, household.approve())
        })
        .await
        .unwrap();
        let (guest, profile) = joined.unwrap().unwrap();
        assert_eq!(sign_in.presented(), presented + 1);
        assert_eq!(profile.unwrap().unwrap().name, "Rowan");
        guest.close().await.unwrap();
        household.owner.close().await.unwrap();
    });
}
