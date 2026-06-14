import Observation

/// Position-update tick interval handed to the bridge, in milliseconds.
/// bae-core drives the playback engine and emits `PlaybackProgress` at this
/// cadence; the progress bar re-renders from each tick.
private let positionUpdateIntervalMs: UInt32 = 200

struct LockedLibrary {
    let library: BridgeLibrary
    let config: BridgeConfig
}

/// The app's lifecycle screen. The locked-library screen carries the bridge
/// records `UnlockView` needs to display and retry the library open.
enum AppScreen {
    case loading
    case onboarding
    case unlock(LockedLibrary)
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
    var activeLibraryId: String? { appService?.configStore.config.libraryId }

    /// Whether to offer the library switcher (more than one local library).
    var hasMultipleLibraries: Bool { libraries.count > 1 }

    /// Whether `library` is the currently-open one — the data-layer derivation
    /// the switcher renders directly instead of comparing ids in the view.
    func isActive(_ library: BridgeLibrary) -> Bool {
        library.id == activeLibraryId
    }

    @ObservationIgnored
    private var openTask: Task<Void, Never>?

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
            screen = .failed(message: error.localizedDescription)
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

    /// Re-open the locked library after UnlockView has stored the key.
    func retryUnlock() {
        guard case let .unlock(lockedLibrary) = screen else {
            preconditionFailure("retryUnlock called while screen is \(screen)")
        }
        openLibrary(lockedLibrary.library)
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
        do {
            try service.appHandle.forgetLibrary()
        }
        catch {
            screen = .failed(message: error.localizedDescription)
            return
        }
        // Drop the handle so ARC runs the Rust destructor and closes the DB
        // whose directory was just removed, before re-discovery spins up again.
        // activeLibraryId follows appService (it's derived), so this clears it.
        appService = nil
        start()
    }

    /// Open `library`: run `initApp` off-main, gate on the encryption key, and
    /// on the happy path build + wire the `AppService` and store the restore
    /// code once sync is ready. The CloudKit driver is installed once at app
    /// startup, so no per-open registration is needed here.
    func openLibrary(_ library: BridgeLibrary) {
        screen = .loading
        openTask?.cancel()
        openTask = Task {
            do {
                // Optional so the encryption-key gate can drop the handle
                // before showing Unlock: the Swift uniffi binding has no
                // `close()` (unlike Kotlin's `AutoCloseable`), so releasing the
                // only strong reference is the teardown — ARC runs the Rust
                // destructor, freeing the tokio runtime + DB before an unlock
                // retry spins a second `initApp` on the same library.
                var handle: AppHandle? = try await Task.detached {
                    try initApp(
                        libraryId: library.id,
                        positionUpdateIntervalMs: positionUpdateIntervalMs
                    )
                }.value
                // A newer openLibrary may have superseded us while initApp ran
                // (cancelling this task). Bail before touching screen/appService
                // so the stale open can't clobber the current one; `handle` drops
                // here, freeing the core it just built.
                try Task.checkCancellation()
                let config = handle!.getConfig()

                if config.encryptionKeyStored, !handle!.hasEncryptionKey() {
                    handle = nil
                    let lockedLibrary = LockedLibrary(
                        library: library,
                        config: config
                    )
                    screen = .unlock(lockedLibrary)
                    return
                }

                let openHandle = handle!
                let service = AppService(appHandle: openHandle, config: config)
                service.wireUp()
                if openHandle.isSyncReady() {
                    service.sync.storeRestoreCodeInKeychain(
                        libraryId: config.libraryId,
                        onError: { message in
                            Task { @MainActor in
                                service.configStore.showError(message)
                            }
                        }
                    )
                }
                openHandle.triggerSync()
                appService = service
                screen = .library(service)
            }
            catch is CancellationError {
                // Superseded by a newer openLibrary; that call owns `screen`.
            }
            catch {
                screen = .failed(message: error.localizedDescription)
            }
        }
    }
}
