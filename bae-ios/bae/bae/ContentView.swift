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
                        libraryId: lockedLibrary.config.libraryId,
                        libraryName: lockedLibrary.config.libraryName,
                        fingerprint: lockedLibrary.config.encryptionKeyFingerprint,
                        onUnlocked: holder.retryUnlock,
                        onCancel: holder.cancelUnlock
                    )

                case .library(let service):
                    LibraryView()
                        .environment(holder)
                        .environment(service)
                        .environment(service.libraryStore)
                        .environment(service.configStore)
                        .environment(service.playbackStore)
                        .environment(service.downloadStore)
                        .environment(service.outboxStore)
                        .environment(service.downloads)
                        .environment(service.projectionRegistry)
                        .environment(service.imageStore)
                        .environment(service.library)
                        .environment(service.playback)
                        .environment(service.queue)
                        .environment(service.sync)

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
            // Persist playback on background so the queue, current track, and
            // position survive process death while suspended. We can't shut core
            // down (that would stop the background audio), so this is the only
            // save point on iOS. Wrapped in a UIKit background task: without
            // it, iOS is free to suspend the process as soon as the scene
            // finishes backgrounding, which can cut this await off before
            // savePlaybackState returns.
            if phase == .background {
                Task { [handle = holder.appService?.appHandle] in
                    let task = BackgroundSaveTask(name: "SavePlaybackState")
                    defer { task.end() }
                    await handle?.savePlaybackState()
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
