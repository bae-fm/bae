import BaeKit
import Foundation
import Observation

/// Position-update tick interval handed to the bridge, in milliseconds.
/// bae-core drives the playback engine and updates its retained playback value
/// at this cadence; the progress bar re-renders from each delivered value.
private let positionUpdateIntervalMs: UInt32 = 200

struct LockedLibrary {
    let library: BridgeLibrary
}

/// The app's lifecycle screen. The locked-library screen carries the bridge
/// records `UnlockView` needs to display and retry the library open.
enum AppScreen {
    case loading
    case onboarding
    case unlock(LockedLibrary)
    /// The OS keychain refused the key read. On iOS this needs the device to
    /// have been unlocked at least once since boot — the app can be launched
    /// into the background before that — so it is rarer than on macOS, and it
    /// is not a dead end: the retry runs when the scene becomes active.
    case keychainLocked(BridgeLibrary)
    case library(AppService)
    case failed(message: String)
}

/// Drives the `AppScreen` lifecycle and owns the open `AppService`. Mirrors the
/// macOS `AppDelegate` / Android `AppSessionHolder`: discover an existing
/// library and open it (gating on the encryption key), or fall through to
/// onboarding. The `initApp` + config read run off the main actor; the built
/// `AppService` and the resulting screen land back on the main actor.
@MainActor
@Observable
final class AppSessionHolder {
    private(set) var screen: AppScreen = .loading

    /// The open library's services, present once a library is unlocked and
    /// wired. ContentView injects these into the view tree via `@Environment`.
    private(set) var appService: AppService?

    /// Local libraries discovered on this device — drives the Settings library
    /// switcher. Refreshed on launch and when the set changes (link).
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

    /// The platform-shared open sequence. iOS builds the service with no
    /// `mediaControlService` / `uiStore` to inject, so the factory just
    /// constructs and wires the service; the opener owns the supersede-cancel
    /// slot and maps each open to an `Outcome` this holder lands on `screen`.
    @ObservationIgnored
    private let opener: LibrarySessionOpener<AppHandle, AppService>

    /// `diagnostics` is the process-lifetime telemetry sink, built at launch
    /// and shared across library opens; the opener's closures capture it for
    /// `init_app` and each `AppService`.
    init(diagnostics: BridgeDiagnostics) {
        opener = LibrarySessionOpener<AppHandle, AppService>(
            makeHandle: { [diagnostics] libraryId in
                try initApp(
                    libraryId: libraryId,
                    positionUpdateIntervalMs: positionUpdateIntervalMs,
                    // The "Restore on launch" preference (default on for mobile):
                    // off starts with nothing in playback; the core keeps the
                    // resume row current either way.
                    restorePlayback: UserDefaults.standard.object(forKey: "persistPlayback") == nil
                        || UserDefaults.standard.bool(forKey: "persistPlayback"),
                    // The telemetry sink built at launch; `init_app` requires it.
                    diagnostics: diagnostics
                )
            },
            makeService: { [diagnostics] handle, config, initialOutbox in
                let service = AppService(
                    appHandle: handle,
                    diagnostics: diagnostics,
                    config: config,
                    initialOutbox: initialOutbox
                )
                service.wireUp()
                return service
            }
        )
    }

    /// On launch: open the first discovered library, or onboard if none exist.
    func start() {
        do {
            libraries = try discoverLibraries()
            guard let first = libraries.first else {
                screen = .onboarding
                return
            }
            openLibrary(first)
        }
        catch {
            // No line means a cancellation — a superseding open owns the screen,
            // so don't paint a blank failure over it.
            guard let message = error.displayLine else { return }
            screen = .failed(message: message)
        }
    }

    /// Called by onboarding once `restoreFromCode` produced a library. Add the
    /// freshly-linked library to the known list (it isn't in the launch scan)
    /// rather than re-scanning the filesystem, then open it.
    func onLinked(_ info: BridgeLibrary) {
        if !libraries.contains(where: { $0.id == info.id }) {
            libraries.append(info)
        }
        openLibrary(info)
    }

    /// Complete the retained locked handle and install its service.
    func unlock(serializedCloudKey: String) async throws {
        let service = try await opener.unlock(
            serializedCloudKey: serializedCloudKey
        )
        appService = service
        screen = .library(service)
        service.reportScreen(.library)
    }

    /// Leave the unlock gate without adding the key.
    func cancelUnlock() {
        if let service = appService {
            screen = .library(service)
        }
        else {
            screen = .onboarding
        }
    }

    /// Forget the active library on this device: delete its key, clear the
    /// active pointer, and remove its files (the cloud copy is untouched). Then
    /// drop the handle and re-discover — opening the next library or onboarding.
    /// Called from Settings.
    func forgetActiveLibrary() {
        guard let service = appService else {
            return
        }
        Task {
            do {
                try await service.forgetLibrary()
            }
            catch {
                guard let message = error.displayLine else { return }
                screen = .failed(message: message)
                return
            }
            appService = nil
            start()
        }
    }

    /// Open `library` through the shared opener and land the outcome on
    /// `screen`. The CloudKit driver is installed once at app startup, so no
    /// per-open registration is needed here.
    func openLibrary(_ library: BridgeLibrary) {
        screen = .loading
        opener.open(libraryId: library.id) { [weak self] outcome in
            guard let self else { return }
            switch outcome {
            case .opened(let service):
                self.appService = service
                self.screen = .library(service)
                service.reportScreen(.library)
            case .needsUnlock:
                self.screen = .unlock(
                    LockedLibrary(library: library)
                )
            case .keychainLocked:
                // Not `.failed`: that is a terminal screen with no way out, and
                // this library is intact. Retried on scene activation.
                logger.info(
                    "Library open deferred: the OS keychain is locked"
                )
                self.screen = .keychainLocked(library)
            case .superseded:
                // A newer open owns `screen`; leave it alone.
                break
            case .failed(let error):
                guard let message = error.displayLine else { break }
                self.screen = .failed(message: message)
            }
        }
    }

    /// Re-attempt an open the keychain refused. A no-op in every other state,
    /// so the scene-activation caller does not have to know what is on screen.
    /// Logged with its trigger: this is event-driven, not a poll.
    func retryOpenIfKeychainWasLocked(trigger: String) {
        guard case .keychainLocked(let library) = screen else { return }
        logger.info(
            "Retrying the library open after \(trigger); the OS keychain may be reachable now"
        )
        openLibrary(library)
    }

    func reportScreen(_ screen: BridgeScreen) {
        appService?.reportScreen(screen)
    }
}
