import BaeKit
import Foundation
import Observation
import os.log

private let logger = Logger.bae("AppSessionHolder")

/// Position-update tick interval handed to the bridge, in milliseconds.
/// bae-core drives the playback engine and updates its retained playback value
/// at this cadence; the progress bar re-renders from each delivered value.
private let positionUpdateIntervalMs: UInt32 = 200

/// The "Restore on launch" preference (default on for mobile): off starts with
/// nothing in playback; the core keeps the resume row current either way.
private func restorePlayback() -> Bool {
    UserDefaults.standard.object(forKey: "persistPlayback") == nil
        || UserDefaults.standard.bool(forKey: "persistPlayback")
}

/// The app's lifecycle screen.
enum AppScreen {
    case loading
    case onboarding
    /// The OS keychain refused the key read. On iOS this needs the device to
    /// have been unlocked at least once since boot — the app can be launched
    /// into the background before that — so it is rarer than on macOS, and it
    /// is not a dead end: the retry runs when the scene becomes active.
    case keychainLocked(libraryId: String)
    /// Opening found a damaged database, or sync found another copy of this
    /// device: the person can reset the device for this library after its
    /// warning. `error` is why it didn't open, or why the last reset failed.
    case resetRequired(libraryId: String, error: DisplayError?)
    /// Another of the person's devices replaced their keys: the library is
    /// closed, and the person restores it with their current restore code.
    case restoreRequired(libraryId: String)
    case library(AppService)
    case failed(message: String)
}

/// Drives the `AppScreen` lifecycle and owns the open `AppService`. Mirrors the
/// macOS `AppDelegate`: list this device's libraries and open one, or fall
/// through to onboarding; open what a restore, join or device reset returned;
/// remove the open library. The `initApp` and seed reads run off the main
/// actor; the built `AppService` and the resulting screen land back on it.
@MainActor
@Observable
final class AppSessionHolder {
    private(set) var screen: AppScreen = .loading

    /// The open library's services, present once a library is open and wired.
    /// ContentView injects these into the view tree via `@Environment`.
    private(set) var appService: AppService?

    /// Libraries on this device — drives the Settings library switcher.
    /// Refreshed on launch and whenever a library opens.
    private(set) var libraries: [BridgeLibrary] = []

    /// Id of the currently-open library, derived from the open service so there
    /// is a single source of truth (the open library's config), letting the
    /// switcher mark the active row.
    var activeLibraryId: String? { appService?.libraryId }

    /// Whether to offer the library switcher (more than one local library).
    var hasMultipleLibraries: Bool { libraries.count > 1 }

    /// Whether `library` is the currently-open one — the data-layer derivation
    /// the switcher renders directly instead of comparing ids in the view.
    func isActive(_ library: BridgeLibrary) -> Bool {
        library.id == activeLibraryId
    }

    /// Restore, join and saved-code listing before a library is open.
    @ObservationIgnored
    let setup: LibrarySetup
    /// Removals of libraries from this device, including one that left the
    /// store but not this device's files for it.
    @ObservationIgnored
    let removals: LibraryRemovals

    /// The platform-shared open sequence; it owns the supersede-cancel slot
    /// and maps each open to an `Outcome` this holder lands on `screen`.
    @ObservationIgnored
    private let opener: LibrarySessionOpener<AppHandle, AppService>
    @ObservationIgnored
    private let host: BridgeHost
    @ObservationIgnored
    private let deviceName: String
    /// In-flight library-list read, cancelled when a newer one supersedes it.
    @ObservationIgnored
    private let listSlot = CancellableTaskSlot()
    /// In-flight device reset or removal of the open library.
    @ObservationIgnored
    private let lifecycleSlot = CancellableTaskSlot()

    /// `diagnostics` is the process-lifetime telemetry sink, `host` the
    /// process-lifetime host registrations, and `deviceName` the name this
    /// device registers under; all three are built at launch and shared across
    /// library opens.
    init(diagnostics: BridgeDiagnostics, host: BridgeHost, deviceName: String) {
        self.host = host
        self.deviceName = deviceName
        setup = LibrarySetup.live(host: host, deviceName: deviceName)
        removals = LibraryRemovals.live(host: host)
        opener = LibrarySessionOpener<AppHandle, AppService>(
            makeHandle: { [host] libraryId in
                try initApp(
                    libraryId: libraryId,
                    positionUpdateIntervalMs: positionUpdateIntervalMs,
                    restorePlayback: restorePlayback(),
                    host: host
                )
            },
            makeService: { [diagnostics] handle, config, seed in
                let service = AppService(
                    appHandle: handle,
                    diagnostics: diagnostics,
                    config: config,
                    seed: seed,
                    deviceName: deviceName
                )
                service.wireUp()
                return service
            }
        )
    }

    /// On launch, and after a removal: open the library core lists first (the
    /// active one), or onboard if this device has none.
    func start() {
        screen = .loading
        let list = setup.listLibraries
        listSlot.replace(
            "listLibraries",
            work: { try await list() },
            onSuccess: { libraries in
                self.libraries = libraries
                guard let first = libraries.first else {
                    self.screen = .onboarding
                    return
                }
                self.openLibrary(id: first.id)
            },
            onError: { error in
                logger.error(
                    "Failed to list libraries: \(error.localizedDescription)"
                )
                guard let message = error.displayLine else { return }
                self.screen = .failed(message: message)
            }
        )
    }

    /// Open one of this device's libraries, showing `notice` once it is up.
    func openLibrary(id libraryId: String, notice: DisplayError? = nil) {
        screen = .loading
        opener.open(libraryId: libraryId) { [weak self] outcome in
            self?.apply(outcome, libraryId: libraryId, notice: notice)
        }
    }

    /// Open the store a restore, join or device reset returned, showing the
    /// error a partly finished one reported once the library is up.
    func openOpenedLibrary(_ library: OpenedLibrary, warning: BridgeError?) {
        screen = .loading
        let libraryId = library.libraryId()
        let restore = restorePlayback()
        opener.open(
            makeHandle: {
                try library.open(
                    positionUpdateIntervalMs: positionUpdateIntervalMs,
                    restorePlayback: restore
                )
            },
            onOutcome: { [weak self] outcome in
                self?
                    .apply(
                        outcome,
                        libraryId: libraryId,
                        notice: warning.flatMap { DisplayError($0) }
                    )
            }
        )
    }

    private func apply(
        _ outcome: LibrarySessionOpener<AppHandle, AppService>.Outcome,
        libraryId: String,
        notice: DisplayError?
    ) {
        switch outcome {
        case .opened(let service):
            appService = service
            screen = .library(service)
            service.reportScreen(.library)
            if let notice {
                service.showError(notice)
            }
            reloadLibraries()
        case .keychainLocked:
            // Not `.failed`: that is a terminal screen with no way out, and
            // this library is intact. Retried on scene activation.
            logger.info("Library open deferred: the OS keychain is locked")
            screen = .keychainLocked(libraryId: libraryId)
        case .superseded:
            // A newer open owns `screen`; leave it alone.
            break
        case .failed(let error):
            if case BridgeError.Diagnostic(.deviceResetRequired, _) = error {
                screen = .resetRequired(
                    libraryId: libraryId,
                    error: DisplayError(error)
                )
            }
            else if let message = error.displayLine {
                screen = .failed(message: message)
            }
        }
    }

    private func reloadLibraries() {
        let list = setup.listLibraries
        listSlot.replace(
            "listLibraries",
            work: { try await list() },
            onSuccess: { self.libraries = $0 },
            onError: {
                // The switcher keeps the last list it had.
                logger.error(
                    "Failed to list libraries: \($0.localizedDescription)"
                )
            }
        )
    }

    /// Re-attempt an open the keychain refused. A no-op in every other state,
    /// so the scene-activation caller does not have to know what is on screen.
    /// Logged with its trigger: this is event-driven, not a poll.
    func retryOpenIfKeychainWasLocked(trigger: String) {
        guard case .keychainLocked(let libraryId) = screen else { return }
        logger.info(
            "Retrying the library open after \(trigger); the OS keychain may be reachable now"
        )
        openLibrary(id: libraryId)
    }

    func reportScreen(_ screen: BridgeScreen) {
        appService?.reportScreen(screen)
    }
}

/// Recovering a library this device can no longer sync: resetting this
/// device, or restoring the library over its copy with the person's code.
extension AppSessionHolder {
    /// Show the device-reset warning for the open library, from Settings'
    /// sync status after sync found another copy of this device.
    func presentDeviceReset() {
        guard let appService else {
            preconditionFailure("sync asks for a reset only while open")
        }
        screen = .resetRequired(libraryId: appService.libraryId, error: nil)
    }

    /// Leave the reset warning without resetting.
    func cancelDeviceReset() {
        if let appService {
            screen = .library(appService)
        }
        else {
            start()
        }
    }

    /// Reset this device for a library: close it if it is open, then load it
    /// from storage again under a fresh device id. The person saw the warning
    /// before choosing, so nothing is shown after.
    func resetDevice(libraryId: String) {
        let open = appService.flatMap { $0.libraryId == libraryId ? $0 : nil }
        opener.cancel()
        screen = .loading
        let host = host
        let deviceName = deviceName
        lifecycleSlot.replace {
            do {
                if let open {
                    try await open.shutdown()
                    try await DetachedWork.run { try open.closeLibrary() }
                    self.appService = nil
                }
                let reset = try await host.resetDevice(
                    libraryId: libraryId,
                    deviceName: deviceName
                )
                self.openOpenedLibrary(reset.library, warning: reset.warning)
            }
            catch {
                logger.error("Device reset failed: \(error.localizedDescription)")
                self.screen = .resetRequired(
                    libraryId: libraryId,
                    error: DisplayError(error)
                )
            }
        }
    }

    /// Close the open library and ask for the person's current restore code,
    /// to restore it over this device's copy after another of their devices
    /// replaced their keys. Going back opens the library again as it was.
    func presentDeviceRestore() {
        guard let open = appService else {
            preconditionFailure("a pending record asks for a restore only while open")
        }
        let libraryId = open.libraryId
        opener.cancel()
        screen = .loading
        lifecycleSlot.replace {
            do {
                try await open.shutdown()
                try await DetachedWork.run { try open.closeLibrary() }
                self.appService = nil
                self.screen = .restoreRequired(libraryId: libraryId)
            }
            catch {
                logger.error(
                    "Closing the library to restore it failed: \(error.localizedDescription)"
                )
                self.openLibrary(id: libraryId, notice: DisplayError(error))
            }
        }
    }
}

// MARK: - Removing the open library

extension AppSessionHolder {
    /// Remove the open library from this device, once the person confirmed
    /// what it loses: it leaves the store's members, then its local copy is
    /// deleted, discarding what this device hasn't sent, and the next library
    /// opens. A failed removal opens the library again.
    func removeActiveLibrary() {
        guard let service = appService else {
            preconditionFailure("Remove shows only while a library is open")
        }
        opener.cancel()
        screen = .loading
        let removals = removals
        lifecycleSlot.replace {
            do {
                let removal = try await service.removeLibrary()
                self.appService = nil
                removals.record(removal)
                self.start()
            }
            catch {
                logger.error(
                    "Failed to remove library: \(error.localizedDescription)"
                )
                await self.reopen(
                    service,
                    notice: DisplayError(error)?
                        .addingContext(
                            String(localized: "Couldn't remove library")
                        )
                )
            }
        }
    }

    /// Removal stopped the library's services before leaving the store; close
    /// what is left and open it again, showing why it stayed.
    private func reopen(_ service: AppService, notice: DisplayError?) async {
        do {
            try await DetachedWork.run { try service.closeLibrary() }
        }
        catch let closeError {
            logger.error(
                "Failed to close the library after its removal stopped: \(closeError.localizedDescription)"
            )
        }
        appService = nil
        openLibrary(id: service.libraryId, notice: notice)
    }
}
