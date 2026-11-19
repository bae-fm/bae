use super::*;
use crate::airplay::digest::DigestStyle;
use crate::airplay::fake_raop_receiver::{Challenges, DigestGate};
use crate::airplay::fake_receiver::{
    listen_for_code_pairing, CodeReceiver, CodeRefusal, ReceiverCode,
};
use crate::airplay::fake_verification_receiver::{listen_for_verification, VerificationReceiver};
use crate::airplay::pairing::public_key;
use crate::renderer::discovery::reported::ReportedRenderer;
use crate::renderer::{RendererDiscovery, RendererServiceType};
use crate::tests::{no_code, set_casting, start_discovery, test_controller};
use bae_playback::{PlaybackProgress, RemotePlayback};
use bae_util::test_channels::HANG_GUARD;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// The receiver's id, as discovery reports it.
const RECEIVER: &str = "AA02";

/// A prompt that answers each request with the next of its codes, and keeps
/// every request it was asked.
struct ScriptedPrompt {
    codes: Mutex<VecDeque<&'static str>>,
    asked: Mutex<Vec<CodeRequest>>,
}

impl ScriptedPrompt {
    fn answering(codes: &[&'static str]) -> Arc<Self> {
        Arc::new(ScriptedPrompt {
            codes: Mutex::new(codes.iter().copied().collect()),
            asked: Mutex::new(Vec::new()),
        })
    }

    fn asked(&self) -> Vec<CodeRequest> {
        self.asked.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl CodePrompt for ScriptedPrompt {
    async fn code(&self, request: CodeRequest) -> Result<String, bae_util::task_failure::Bug> {
        self.asked.lock().unwrap().push(request);
        let code = self.codes.lock().unwrap().pop_front();
        Ok(code.expect("the test scripted a code for every ask").to_string())
    }
}

/// An AirPlay 2 service at `127.0.0.1:port` whose status flags are `flags`.
fn reported_receiver(port: u16, flags: &str) -> ReportedRenderer {
    ReportedRenderer {
        service_type: RendererServiceType::AirPlay,
        instance_name: "Kitchen".to_string(),
        addr: "127.0.0.1".to_string(),
        port,
        txt: [("deviceid", RECEIVER), ("features", "0x0,0x10000"), ("flags", flags)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

/// The AirPlay 1 receiver's id, from the MAC that begins its RAOP name.
const RAOP_RECEIVER: &str = "001122334455";

/// A RAOP service at `127.0.0.1:port` whose status flags are `flags`.
fn reported_raop_receiver(port: u16, flags: &str) -> ReportedRenderer {
    ReportedRenderer {
        service_type: RendererServiceType::Raop,
        instance_name: format!("{RAOP_RECEIVER}@Studio"),
        addr: "127.0.0.1".to_string(),
        port,
        txt: [("et", "0"), ("sf", flags), ("am", "AirPort10,115")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

/// A controller with casting on and the picker open, listing one receiver
/// that `receiver` serves.
struct Casting<R> {
    runtime: tokio::runtime::Runtime,
    controller: Arc<CastController>,
    services: bae_app::AppServices,
    receiver: Arc<Mutex<R>>,
    /// The receiver's id, which the cast names.
    id: &'static str,
    _tmp: bae_util::test_files::TestFolder,
}

impl<R> Casting<R> {
    /// List the receiver `reported` gives for the port it is served at.
    fn listing(
        id: &'static str,
        (port, receiver): (u16, Arc<Mutex<R>>),
        reported: impl FnOnce(u16) -> ReportedRenderer,
    ) -> Self {
        let (runtime, controller, services, tmp) = test_controller(RendererDiscovery::reported());
        set_casting(&runtime, &controller, &services, true);
        start_discovery(&runtime, &controller);
        controller.renderer_found(reported(port));
        Casting { runtime, controller, services, receiver, id, _tmp: tmp }
    }

    fn cast(&self, prompt: Arc<dyn CodePrompt>) -> Result<(), CastError> {
        self.runtime.block_on(async {
            tokio::time::timeout(HANG_GUARD, self.controller.cast_to(self.id, prompt))
                .await
                .expect("the cast ends")
        })
    }
}

impl Casting<CodeReceiver> {
    /// An AirPlay 2 receiver with status `flags`.
    fn to(receiver: CodeReceiver, flags: &str) -> Self {
        Self::listing(RECEIVER, listen_for_code_pairing(receiver), |port| {
            reported_receiver(port, flags)
        })
    }

    /// The pairing this device kept for the receiver.
    fn kept(&self) -> Option<KeptPairing> {
        self.controller
            .device_secrets
            .device_secret(&pairing_secret_name(RECEIVER))
            .unwrap()
            .map(|text| KeptPairing::decode(&text).unwrap())
    }

    /// The URIs of the requests the receiver took, in order.
    fn requests(&self) -> Vec<String> {
        self.receiver.lock().unwrap().requests.clone()
    }
}

fn first_pin(why: CodeAsked) -> CodeRequest {
    CodeRequest { code: AIRPLAY_PIN, why, last_try: None }
}

/// A receiver that asks for a PIN once: the first cast asks for it, pairs,
/// keeps the pairing on this device and moves playback; the next cast
/// pair-verifies with what it kept and asks nothing.
#[test]
fn a_pin_once_receiver_asks_on_the_first_cast_only() {
    let casting = Casting::to(CodeReceiver::new(ReceiverCode::pins(&["0482"])), "0x204");
    let mut progress = casting.services.subscribe_playback_progress();
    let prompt = ScriptedPrompt::answering(&["0482"]);
    casting.cast(prompt.clone()).expect("the receiver took the PIN");
    assert_eq!(prompt.asked(), [first_pin(CodeAsked::FirstPairing)]);
    let kept = casting.kept().expect("the cast kept the pairing");
    assert_eq!(kept.password, None);
    casting.runtime.block_on(async {
        tokio::time::timeout(HANG_GUARD, async {
            while let Some(event) = progress.recv().await {
                if matches!(
                    event,
                    PlaybackProgress::RemoteStatusChanged { remote: RemotePlayback::On(_) }
                ) {
                    return;
                }
            }
            panic!("the progress feed ended before playback moved to the receiver");
        })
        .await
        .expect("playback moves to the receiver");
    });

    let again = ScriptedPrompt::answering(&[]);
    casting.cast(again.clone()).expect("the kept pairing verifies");
    assert_eq!(again.asked(), []);
    assert_eq!(casting.kept(), Some(kept));
}

/// A receiver that no longer knows the pairing bae kept asks for the PIN
/// with why, and the new pairing replaces the kept one.
#[test]
fn a_forgotten_pairing_asks_again_and_replaces_what_bae_kept() {
    let casting = Casting::to(CodeReceiver::new(ReceiverCode::pins(&["1111", "2222"])), "0x204");
    casting.cast(ScriptedPrompt::answering(&["1111"])).expect("the first pairing");
    let first = casting.kept().unwrap();
    casting.receiver.lock().unwrap().forget_paired();

    let prompt = ScriptedPrompt::answering(&["2222"]);
    casting.cast(prompt.clone()).expect("the new pairing");
    assert_eq!(prompt.asked(), [first_pin(CodeAsked::PairingLost)]);
    let second = casting.kept().unwrap();
    assert_ne!(second.keys.controller, first.keys.controller, "the new pairing replaced the old");
}

/// A pairing by code already proved this device and the receiver to each
/// other, so bae hands its keys to playback without pair-verifying them
/// first: a receiver that refuses every pair-verify still takes the cast,
/// and only the session's own pair-verify reaches it.
#[test]
fn a_pairing_by_code_moves_playback_without_verifying_again() {
    let mut receiver = CodeReceiver::new(ReceiverCode::pins(&["0482"]));
    receiver.refusal = Some(CodeRefusal::VerifyM2(0x02));
    let casting = Casting::to(receiver, "0x204");
    casting
        .cast(ScriptedPrompt::answering(&["0482"]))
        .expect("the cast moves playback with the keys the pairing agreed on");
    assert!(casting.kept().is_some());
}

/// A receiver that asks for a PIN on every cast asks on each, and bae keeps
/// nothing of it on this device.
#[test]
fn a_pin_every_cast_receiver_asks_every_cast_and_keeps_nothing() {
    let casting = Casting::to(CodeReceiver::new(ReceiverCode::pins(&["1111", "2222"])), "0x8");
    for pin in ["1111", "2222"] {
        let prompt = ScriptedPrompt::answering(&[pin]);
        casting.cast(prompt.clone()).expect("the receiver took the PIN");
        assert_eq!(prompt.asked(), [first_pin(CodeAsked::EveryCast)]);
    }
    assert_eq!(casting.kept(), None);
}

/// A password receiver asks for its password with no PIN start, and bae
/// keeps the password beside the keys for the Digest challenge each session
/// answers.
#[test]
fn a_password_receiver_asks_for_its_password_and_keeps_it() {
    let casting = Casting::to(CodeReceiver::new(ReceiverCode::Password("hunter2".into())), "0x80");
    let prompt = ScriptedPrompt::answering(&["hunter2"]);
    casting.cast(prompt.clone()).expect("the receiver took the password");
    assert_eq!(
        prompt.asked(),
        [CodeRequest { code: CodeKind::Password, why: CodeAsked::FirstPairing, last_try: None }]
    );
    assert!(!casting.requests().iter().any(|uri| uri == "/pair-pin-start"));
    assert_eq!(casting.kept().unwrap().password.as_deref(), Some("hunter2"));
}

/// A wrong PIN asks again with what was wrong, after a new PIN start, which
/// shows a new PIN; a code of the wrong shape asks again without reaching
/// the receiver.
#[test]
fn a_wrong_pin_or_a_wrong_shape_asks_again() {
    let casting = Casting::to(CodeReceiver::new(ReceiverCode::pins(&["1111", "2222"])), "0x204");
    let prompt = ScriptedPrompt::answering(&["9999", "22", "2222"]);
    casting.cast(prompt.clone()).expect("the second PIN pairs");
    let request = |last_try| CodeRequest {
        code: AIRPLAY_PIN,
        why: CodeAsked::FirstPairing,
        last_try: Some(last_try),
    };
    assert_eq!(
        prompt.asked(),
        [
            first_pin(CodeAsked::FirstPairing),
            request(CodeRefused::WrongCode),
            request(CodeRefused::WrongShape),
        ]
    );
    let requests = casting.requests();
    assert_eq!(requests.iter().filter(|uri| *uri == "/pair-pin-start").count(), 2, "{requests:?}");
    assert_eq!(
        requests.iter().filter(|uri| *uri == "/pair-setup").count(),
        5,
        "M1 and M3 of the wrong PIN, M1, M3 and M5 of the right one: {requests:?}"
    );
}

/// After a wrong PIN whose reply asks to wait, bae sends the next PIN only
/// once the wait has passed: the receiver answers an earlier one with
/// BackOff, which would fail the cast.
#[test]
fn the_next_code_waits_out_the_retry_delay() {
    let mut receiver = CodeReceiver::new(ReceiverCode::pins(&["1111", "2222"]));
    receiver.retry_delay = Some(1);
    let casting = Casting::to(receiver, "0x204");
    casting
        .cast(ScriptedPrompt::answering(&["9999", "2222"]))
        .expect("the second PIN went after the wait");
}

/// A receiver that answers transient pair-setup with 470 asks for a PIN, as
/// one whose flags ask for it once.
#[test]
fn a_470_to_transient_pairing_asks_for_a_pin() {
    let casting = Casting::to(CodeReceiver::new(ReceiverCode::pins(&["4821"])), "0x4");
    let prompt = ScriptedPrompt::answering(&["4821"]);
    casting.cast(prompt.clone()).expect("the receiver took the PIN");
    assert_eq!(prompt.asked(), [first_pin(CodeAsked::FirstPairing)]);
    assert!(casting.kept().is_some());
}

/// A Mac whose AirPlay Receiver plays only for its current user answers the
/// PIN start with 400, which is the owner-only refusal, naming the receiver.
#[test]
fn a_400_to_the_pin_start_plays_only_from_the_owner() {
    let mut receiver = CodeReceiver::new(ReceiverCode::pins(&["4821"]));
    receiver.refusal = Some(CodeRefusal::PinStart400);
    let casting = Casting::to(receiver, "0x204");
    let prompt = ScriptedPrompt::answering(&[]);
    let cast = casting.cast(prompt.clone());
    assert!(
        matches!(&cast, Err(CastError::AirPlayOwnerOnly { receiver }) if receiver == "Kitchen"),
        "{cast:?}"
    );
    assert_eq!(prompt.asked(), []);
}

/// Each refusal of a pairing by code fails the cast with its own error,
/// naming the receiver.
#[test]
fn a_refused_code_pairing_names_the_receiver() {
    let mut receiver = CodeReceiver::new(ReceiverCode::pins(&["4821"]));
    receiver.refusal = Some(CodeRefusal::SetupM2(0x05));
    let casting = Casting::to(receiver, "0x204");
    let cast = casting.cast(ScriptedPrompt::answering(&["4821"]));
    assert!(
        matches!(&cast, Err(CastError::AirPlayLockedOut { receiver }) if receiver == "Kitchen"),
        "{cast:?}"
    );
    assert_eq!(casting.kept(), None);
}

/// A prompt that never answers, and says whether its future was dropped.
struct WaitingPrompt {
    asked: tokio::sync::Notify,
    dropped: Arc<AtomicBool>,
}

struct SetOnDrop(Arc<AtomicBool>);

impl Drop for SetOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[async_trait::async_trait]
impl CodePrompt for WaitingPrompt {
    async fn code(&self, _request: CodeRequest) -> Result<String, bae_util::task_failure::Bug> {
        let _dropped = SetOnDrop(self.dropped.clone());
        self.asked.notify_one();
        std::future::pending().await
    }
}

/// Stopping casting while the prompt waits calls the cast off: the prompt's
/// future is dropped, which closes the sheet, and playback stays.
#[test]
fn a_cast_called_off_while_the_prompt_waits_drops_the_prompt() {
    let casting = Casting::to(CodeReceiver::new(ReceiverCode::pins(&["4821"])), "0x204");
    let prompt = Arc::new(WaitingPrompt {
        asked: tokio::sync::Notify::new(),
        dropped: Arc::new(AtomicBool::new(false)),
    });
    let cast = casting.runtime.block_on(async {
        let call = casting.controller.cast_to(RECEIVER, prompt.clone());
        let stop = async {
            tokio::time::timeout(HANG_GUARD, prompt.asked.notified()).await.expect("the cast asks");
            casting.controller.stop_casting();
        };
        let (cast, ()) = tokio::time::timeout(HANG_GUARD, async { tokio::join!(call, stop) })
            .await
            .expect("the cast ends");
        cast
    });
    assert!(matches!(cast, Err(CastError::CalledOff)), "{cast:?}");
    assert!(prompt.dropped.load(Ordering::Acquire), "the prompt's future was dropped");
    assert_eq!(casting.kept(), None);
}

impl Casting<VerificationReceiver> {
    /// An AirPlay 1 receiver with status `flags`.
    fn to_raop(receiver: VerificationReceiver, flags: &str) -> Self {
        Self::listing(RAOP_RECEIVER, listen_for_verification(receiver), |port| {
            reported_raop_receiver(port, flags)
        })
    }

    /// An AirPlay 1 receiver with status `flags` that states no model, as a
    /// first-generation AirPort Express does.
    fn to_raop_without_a_model(receiver: VerificationReceiver, flags: &str) -> Self {
        Self::listing(RAOP_RECEIVER, listen_for_verification(receiver), |port| {
            let mut reported = reported_raop_receiver(port, flags);
            reported.txt.remove("am");
            reported
        })
    }

    /// The password this device kept for the receiver.
    fn kept_password(&self) -> Option<String> {
        self.controller.device_secrets.device_secret(&password_secret_name(RAOP_RECEIVER)).unwrap()
    }

    /// What the receiver's password check took: each request's method and the
    /// status it answered.
    fn password_checks(&self) -> Vec<(String, u16)> {
        let receiver = self.receiver.lock().unwrap();
        let gate = receiver.password.as_ref().expect("the receiver has a password");
        gate.taken.iter().map(|taken| (taken.method.clone(), taken.status)).collect()
    }

    /// The key this device kept for the receiver.
    fn kept_key(&self) -> Option<VerificationKey> {
        self.controller
            .device_secrets
            .device_secret(&verification_secret_name(RAOP_RECEIVER))
            .unwrap()
            .map(|text| VerificationKey::decode(&text).unwrap())
    }

    /// How many requests the receiver took to `uri`.
    fn count(&self, uri: &str) -> usize {
        self.receiver.lock().unwrap().count(uri)
    }
}

/// An AirPlay 1 receiver that verifies the device and asks for its PIN once:
/// the first cast asks for the PIN and keeps the key the receiver took; the
/// next cast verifies that key and asks nothing.
#[test]
fn an_airplay1_receiver_asks_for_its_pin_on_the_first_cast_only() {
    let casting = Casting::to_raop(VerificationReceiver::new(&["0482"]), "0x200");
    let prompt = ScriptedPrompt::answering(&["0482"]);
    casting.cast(prompt.clone()).expect("the receiver took the PIN");
    assert_eq!(prompt.asked(), [first_pin(CodeAsked::FirstPairing)]);
    let kept = casting.kept_key().expect("the cast kept the key");
    assert_eq!(casting.receiver.lock().unwrap().paired, [public_key(&kept.seed)]);
    let this_device = casting.runtime.block_on(casting.services.this_device_name()).unwrap();
    assert_eq!(
        casting.receiver.lock().unwrap().shown_beside,
        Some(this_device),
        "the receiver shows this device's name beside the PIN"
    );

    let again = ScriptedPrompt::answering(&[]);
    casting.cast(again.clone()).expect("the kept key verifies");
    assert_eq!(again.asked(), []);
    assert_eq!(casting.kept_key(), Some(kept));
    assert_eq!(casting.count("/pair-verify"), 2, "the second cast verified the kept key");
}

/// An AirPlay 1 receiver that no longer knows the kept key asks for the PIN
/// with why, and the new key replaces the kept one.
#[test]
fn an_airplay1_receiver_that_forgot_the_key_asks_again() {
    let casting = Casting::to_raop(VerificationReceiver::new(&["1111", "2222"]), "0x200");
    casting.cast(ScriptedPrompt::answering(&["1111"])).expect("the first pairing");
    let first = casting.kept_key().unwrap();
    casting.receiver.lock().unwrap().forget_paired();

    let prompt = ScriptedPrompt::answering(&["2222"]);
    casting.cast(prompt.clone()).expect("the new pairing");
    assert_eq!(prompt.asked(), [first_pin(CodeAsked::PairingLost)]);
    assert_ne!(casting.kept_key().unwrap(), first, "the new key replaced the old");
}

/// An AirPlay 1 receiver that asks for a PIN on every cast asks on each, and
/// bae keeps nothing of it on this device.
#[test]
fn an_airplay1_receiver_that_asks_every_cast_keeps_nothing() {
    let casting = Casting::to_raop(VerificationReceiver::new(&["1111", "2222"]), "0x8");
    for pin in ["1111", "2222"] {
        let prompt = ScriptedPrompt::answering(&[pin]);
        casting.cast(prompt.clone()).expect("the receiver took the PIN");
        assert_eq!(prompt.asked(), [first_pin(CodeAsked::EveryCast)]);
    }
    assert_eq!(casting.kept_key(), None);
}

/// A wrong PIN asks again with what was wrong, after a new PIN start, which
/// shows a new PIN.
#[test]
fn a_wrong_airplay1_pin_asks_again_after_a_new_pin() {
    let casting = Casting::to_raop(VerificationReceiver::new(&["1111", "2222"]), "0x200");
    let prompt = ScriptedPrompt::answering(&["9999", "2222"]);
    casting.cast(prompt.clone()).expect("the second PIN pairs");
    assert_eq!(
        prompt.asked(),
        [
            first_pin(CodeAsked::FirstPairing),
            CodeRequest {
                code: AIRPLAY_PIN,
                why: CodeAsked::FirstPairing,
                last_try: Some(CodeRefused::WrongCode),
            },
        ]
    );
    assert_eq!(casting.count("/pair-pin-start"), 2);
    assert!(casting.kept_key().is_some());
}

/// An AirPlay 1 receiver whose advertisement asks for nothing but that
/// answers OPTIONS with 403 asks for device verification, as one whose
/// flags ask for it once.
#[test]
fn a_403_to_options_asks_for_device_verification() {
    let mut receiver = VerificationReceiver::new(&["4821"]);
    receiver.forbids_unverified = true;
    let casting = Casting::to_raop(receiver, "0x4");
    let prompt = ScriptedPrompt::answering(&["4821"]);
    casting.cast(prompt.clone()).expect("the receiver took the PIN");
    assert_eq!(prompt.asked(), [first_pin(CodeAsked::FirstPairing)]);
    assert!(casting.kept_key().is_some());
}

/// An AirPlay 1 receiver that asks for nothing takes OPTIONS, and the cast
/// asks for nothing and keeps nothing.
#[test]
fn an_airplay1_receiver_that_asks_for_nothing_pairs_not_at_all() {
    let casting = Casting::to_raop(VerificationReceiver::new(&[]), "0x4");
    casting.cast(no_code()).expect("the cast moves playback");
    assert_eq!(casting.kept_key(), None);
    assert_eq!(casting.count("/pair-pin-start") + casting.count("/pair-verify"), 0);
}

/// An AirPlay 1 receiver with a password that has `password` set on it and
/// asks for it as `challenges` says.
fn with_password(
    mut receiver: VerificationReceiver,
    password: &str,
    style: DigestStyle,
    challenges: Challenges,
) -> VerificationReceiver {
    receiver.password = Some(DigestGate::new(password, style, challenges));
    receiver
}

fn password_ask(why: CodeAsked, last_try: Option<CodeRefused>) -> CodeRequest {
    CodeRequest { code: CodeKind::Password, why, last_try }
}

/// An AirPlay 1 receiver that challenges OPTIONS: the first cast asks for the
/// password, answers the 401 with it and sends OPTIONS again, keeps it, and
/// the next cast answers with the kept one and asks nothing.
#[test]
fn an_airplay1_receiver_that_challenges_options_pairs_once_the_password_is_entered() {
    let receiver = with_password(
        VerificationReceiver::new(&[]),
        "hunter2",
        DigestStyle::Standard,
        Challenges::EveryRequest,
    );
    let casting = Casting::to_raop(receiver, "0x80");
    let prompt = ScriptedPrompt::answering(&["hunter2"]);
    casting.cast(prompt.clone()).expect("the receiver took the password");
    assert_eq!(prompt.asked(), [password_ask(CodeAsked::FirstPairing, None)]);
    assert_eq!(casting.kept_password().as_deref(), Some("hunter2"));
    assert_eq!(
        casting.password_checks(),
        [
            ("OPTIONS".to_string(), 401),
            ("OPTIONS".to_string(), 401),
            ("OPTIONS".to_string(), 200),
            ("ANNOUNCE".to_string(), 200)
        ],
        "the probe for unadvertised device verification got a 401, then the 401 to OPTIONS was \
         answered and the request sent again"
    );

    let again = ScriptedPrompt::answering(&[]);
    casting.cast(again.clone()).expect("the kept password is taken");
    assert_eq!(again.asked(), []);
}

/// An AirPlay 1 receiver that challenges only the session's requests is
/// answered at ANNOUNCE.
#[test]
fn an_airplay1_receiver_that_challenges_announce_pairs_once_the_password_is_entered() {
    let receiver = with_password(
        VerificationReceiver::new(&[]),
        "hunter2",
        DigestStyle::Standard,
        Challenges::Session,
    );
    let casting = Casting::to_raop(receiver, "0x80");
    casting.cast(ScriptedPrompt::answering(&["hunter2"])).expect("the receiver took the password");
    assert_eq!(casting.kept_password().as_deref(), Some("hunter2"));
    let receiver = casting.receiver.lock().unwrap();
    assert_eq!(receiver.password.as_ref().unwrap().methods_answered(401), ["ANNOUNCE"]);
}

/// A wrong password asks again with what was wrong and keeps nothing until
/// the receiver takes one. A password of the wrong shape asks again without
/// reaching the receiver.
#[test]
fn a_wrong_airplay1_password_or_a_wrong_shape_asks_again() {
    let receiver = with_password(
        VerificationReceiver::new(&[]),
        "hunter2",
        DigestStyle::Standard,
        Challenges::EveryRequest,
    );
    let casting = Casting::to_raop(receiver, "0x80");
    let prompt = ScriptedPrompt::answering(&["", "hunter3", "hunter2"]);
    casting.cast(prompt.clone()).expect("the third try is the password");
    assert_eq!(
        prompt.asked(),
        [
            password_ask(CodeAsked::FirstPairing, None),
            password_ask(CodeAsked::FirstPairing, Some(CodeRefused::WrongShape)),
            password_ask(CodeAsked::FirstPairing, Some(CodeRefused::WrongCode)),
        ]
    );
    assert_eq!(casting.kept_password().as_deref(), Some("hunter2"));
    let receiver = casting.receiver.lock().unwrap();
    assert_eq!(
        receiver.password.as_ref().unwrap().methods_answered(401),
        ["OPTIONS", "OPTIONS", "OPTIONS", "OPTIONS"],
        "the probe for device verification got a 401; the empty password never reached the \
         receiver; the wrong one got a challenge and a second 401, and the right one a challenge"
    );
}

/// A receiver whose password changed refuses the one bae kept with a second
/// 401, so bae asks again, saying the receiver no longer knows this device,
/// and the new password replaces the kept one.
#[test]
fn a_changed_airplay1_password_asks_again_and_replaces_what_bae_kept() {
    let receiver = with_password(
        VerificationReceiver::new(&[]),
        "hunter2",
        DigestStyle::Standard,
        Challenges::EveryRequest,
    );
    let casting = Casting::to_raop(receiver, "0x80");
    casting.cast(ScriptedPrompt::answering(&["hunter2"])).expect("the first cast");
    casting.receiver.lock().unwrap().password.as_mut().unwrap().set_password("correct horse");

    let prompt = ScriptedPrompt::answering(&["correct horse"]);
    casting.cast(prompt.clone()).expect("the new password");
    assert_eq!(prompt.asked(), [password_ask(CodeAsked::PairingLost, None)]);
    assert_eq!(casting.kept_password().as_deref(), Some("correct horse"));
}

/// A first-generation AirPort Express states no model, and wants the
/// username `iTunes` and capital hex.
#[test]
fn a_first_generation_airport_express_is_answered_in_its_own_style() {
    let receiver = with_password(
        VerificationReceiver::new(&[]),
        "hunter2",
        DigestStyle::AirPortExpress,
        Challenges::EveryRequest,
    );
    let casting = Casting::to_raop_without_a_model(receiver, "0x80");
    casting.cast(ScriptedPrompt::answering(&["hunter2"])).expect("the receiver took the password");
    assert_eq!(casting.kept_password().as_deref(), Some("hunter2"));
}

/// A receiver that verifies the device and has a password gets the PIN
/// prompt and then the password prompt, and bae keeps both.
#[test]
fn an_airplay1_receiver_with_a_pin_and_a_password_asks_for_both_in_turn() {
    let receiver = with_password(
        VerificationReceiver::new(&["4821"]),
        "hunter2",
        DigestStyle::Standard,
        Challenges::Session,
    );
    let casting = Casting::to_raop(receiver, "0x280");
    let prompt = ScriptedPrompt::answering(&["4821", "hunter2"]);
    casting.cast(prompt.clone()).expect("the receiver took both");
    assert_eq!(
        prompt.asked(),
        [first_pin(CodeAsked::FirstPairing), password_ask(CodeAsked::FirstPairing, None)]
    );
    assert!(casting.kept_key().is_some());
    assert_eq!(casting.kept_password().as_deref(), Some("hunter2"));

    let again = ScriptedPrompt::answering(&[]);
    casting.cast(again.clone()).expect("the kept key and password are taken");
    assert_eq!(again.asked(), []);
}
