#![cfg(feature = "test-utils")]

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
    assert!(people[0].membership.as_ref().unwrap().is_self);
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

#[tokio::test]
async fn a_profile_names_its_avatar_once_the_upload_completes() {
    let directory = tempfile::tempdir().unwrap();
    let (manager, database, storage) =
        storage_library(directory.path(), bae_core::providers::Providers::offline()).await;
    connect_storage(&manager, &storage).await;
    let profile = manager.create_profile("Rowan").await.unwrap();
    assert_eq!(manager.avatar(profile.id).await.unwrap(), None);
    manager
        .set_avatar(Some(b"first avatar".to_vec()))
        .await
        .unwrap();
    let first = database.profile(profile.id).await.unwrap().unwrap();
    assert_eq!(
        manager.avatar(profile.id).await.unwrap().unwrap(),
        b"first avatar"
    );
    assert_eq!(
        database
            .queued_upload_count_in_table_for_test("people")
            .await
            .unwrap(),
        1,
        "the avatar waits to upload"
    );
    manager
        .set_avatar(Some(b"second avatar".to_vec()))
        .await
        .unwrap();
    let second = database.profile(profile.id).await.unwrap().unwrap();
    // The profile row names a new avatar only once its upload completes; this
    // device reads its own new avatar before then.
    assert_eq!(first.avatar_id, None);
    assert_eq!(second.avatar_id, None);
    assert_eq!(
        manager.avatar(profile.id).await.unwrap().unwrap(),
        b"second avatar"
    );
    manager.drain_uploads_for_test().await.unwrap();
    let stored = database.profile(profile.id).await.unwrap().unwrap();
    assert!(stored.avatar_id.is_some(), "the uploaded avatar is named");
    assert_eq!(
        manager.avatar(profile.id).await.unwrap().unwrap(),
        b"second avatar"
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
        assert_eq!(database.profile(rowan.id).await.unwrap(), Some(rowan));
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
