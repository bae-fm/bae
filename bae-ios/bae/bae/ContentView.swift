import BaeKit
import SwiftUI
import UIKit

/// App root. Drives the `AppScreen` lifecycle: discover an existing library and
/// open it, gate on the encryption key, or onboard. Once a library is open, the
/// `LibraryView` browses it.
struct ContentView: View {
    // The host's OAuth client config, forwarded to onboarding. Present only in a
    // full build; baeium compiles out the OAuth link flow.
    #if BAE_OAUTH_PROVIDERS
    let oauthLinking: OAuthLinking?
    let oauthLinkingError: String?
    #endif
    let startupError: String?

    @State
    private var holder: AppSessionHolder
    @State
    private var started = false
    @Environment(\.scenePhase)
    private var scenePhase

    #if BAE_OAUTH_PROVIDERS
    @MainActor
    init(
        oauthLinking: OAuthLinking?,
        oauthLinkingError: String?,
        startupError: String?,
        diagnostics: BridgeDiagnostics
    ) {
        self.oauthLinking = oauthLinking
        self.oauthLinkingError = oauthLinkingError
        self.startupError = startupError
        _holder = State(initialValue: AppSessionHolder(diagnostics: diagnostics))
    }
    #else
    @MainActor
    init(startupError: String?, diagnostics: BridgeDiagnostics) {
        self.startupError = startupError
        _holder = State(initialValue: AppSessionHolder(diagnostics: diagnostics))
    }
    #endif

    var body: some View {
        Group {
            if let startupError {
                errorView(startupError)
            }
            else {
                switch holder.screen {
                case .loading:
                    ProgressView()
                        .frame(maxWidth: .infinity, maxHeight: .infinity)

                case .onboarding:
                    #if BAE_OAUTH_PROVIDERS
                    OnboardingView(
                        oauthLinking: oauthLinking,
                        oauthLinkingError: oauthLinkingError,
                        onLinked: holder.onLinked
                    )
                    #else
                    OnboardingView(onLinked: holder.onLinked)
                    #endif

                case .unlock(let lockedLibrary):
                    UnlockView(
                        libraryName: lockedLibrary.library.name,
                        onUnlock: holder.unlock,
                        onCancel: holder.cancelUnlock
                    )

                case .library(let service):
                    service.installEnvironment(
                        VStack(spacing: 0) {
                            ArtworkLoadingBanner()
                            LibraryView()
                        }
                        .environment(holder)
                    )

                case .keychainLocked:
                    // Core owns the sentence — the same one every other surface
                    // shows for this failure. Nothing to type here; the retry
                    // runs on scene activation, and the button covers the case
                    // where that was not what changed.
                    VStack(spacing: 16) {
                        Image(systemName: "lock.fill")
                            .font(.system(size: 40))
                            .foregroundStyle(.secondary)
                        Text("Library Locked")
                            .font(.title2.bold())
                        Text(BridgeErrorCategory.keyringLocked.localizedLine)
                            .font(.callout)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                        Button("Try again") {
                            holder.retryOpenIfKeychainWasLocked(
                                trigger: "the retry button"
                            )
                        }
                        .buttonStyle(.borderedProminent)
                    }
                    .padding()

                case .failed(let message):
                    errorView(message)
                }
            }
        }
        .background(Theme.background)
        .task {
            guard startupError == nil else {
                return
            }
            guard !started else {
                return
            }
            started = true
            holder.start()
        }
        .onChange(of: scenePhase) { _, phase in
            // Returning to the foreground means the device was unlocked, which
            // is exactly the condition a refused keychain read was waiting for.
            //
            // Scene activation is the whole trigger set on iOS, deliberately —
            // there is no `protectedDataDidBecomeAvailable` observer here and
            // that is not an oversight. That notification is only reliably
            // useful to a process that is already running while the device is
            // locked, i.e. a background launch before the first unlock since
            // boot; bae has no background launch path that opens a library. Any
            // unlock a user is present for brings the scene back to `.active`
            // and lands here anyway, so an observer would add a second route to
            // the same retry without covering a case this one misses.
            if phase == .active {
                holder.retryOpenIfKeychainWasLocked(
                    trigger: "the scene becoming active"
                )
            }
            // Persist playback on background so the queue, current track, and
            // position survive process death while suspended. We can't shut core
            // down (that would stop the background audio), so this is the only
            // save point on iOS. Wrapped in a UIKit background task: without
            // it, iOS is free to suspend the process as soon as the scene
            // finishes backgrounding, which can cut this await off before
            // savePlaybackState returns.
            if phase == .background {
                Task { [service = holder.appService] in
                    let task = BackgroundSaveTask(name: "SavePlaybackState")
                    defer { task.end() }
                    do {
                        try await service?.savePlaybackState()
                    }
                    catch {
                        service?.showError(error)
                    }
                }
            }
        }
    }

    private func errorView(_ message: String) -> some View {
        Text(message)
            .foregroundStyle(.red)
            .padding(32)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Owns one UIKit background task and ends its identifier exactly once —
/// whether the work finishes first (the caller's `end()`) or iOS fires the
/// expiration handler because the save overran the background window. Ending
/// an already-ended identifier is an over-release, so `end()` is guarded and
/// clears the identifier after ending; both paths route through it. The
/// identifier lives on the main actor (UIKit requires it), and the expiration
/// handler — invoked by UIKit on the main thread — hops back onto it.
@MainActor
private final class BackgroundSaveTask {
    private var id: UIBackgroundTaskIdentifier = .invalid

    init(name: String) {
        id = UIApplication.shared.beginBackgroundTask(withName: name) {
            [weak self] in
            MainActor.assumeIsolated { self?.end() }
        }
    }

    func end() {
        guard id != .invalid else { return }
        UIApplication.shared.endBackgroundTask(id)
        id = .invalid
    }
}
