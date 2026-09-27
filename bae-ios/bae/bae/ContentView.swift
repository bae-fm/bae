import BaeKit
import SwiftUI
import UIKit

/// App root: opens an existing library, asks for its key, or onboards.
struct ContentView: View {
    // The host's OAuth client config for onboarding; absent in baeium builds.
    #if BAE_OAUTH_PROVIDERS
    let oauthLinking: OAuthLinking?
    let oauthLinkingError: String?
    #endif
    let startupError: String?
    let host: BridgeHost

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
        diagnostics: BridgeDiagnostics,
        host: BridgeHost
    ) {
        self.oauthLinking = oauthLinking
        self.oauthLinkingError = oauthLinkingError
        self.startupError = startupError
        self.host = host
        _holder = State(
            initialValue: AppSessionHolder(diagnostics: diagnostics, host: host)
        )
    }
    #else
    @MainActor
    init(
        startupError: String?,
        diagnostics: BridgeDiagnostics,
        host: BridgeHost
    ) {
        self.startupError = startupError
        self.host = host
        _holder = State(
            initialValue: AppSessionHolder(diagnostics: diagnostics, host: host)
        )
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
                        host: host,
                        oauthLinking: oauthLinking,
                        oauthLinkingError: oauthLinkingError,
                        onLinked: holder.onLinked
                    )
                    #else
                    OnboardingView(host: host, onLinked: holder.onLinked)
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
                    // The open retries on scene activation; the button covers
                    // any other change that unlocked the keychain.
                    VStack(spacing: ThemeSpace.edge) {
                        Image(systemName: "lock.fill")
                            .themeIcon(.hero)
                            .foregroundStyle(.secondary)
                        Text("Library Locked")
                            .themeText(.title)
                        Text(BridgeErrorCategory.keyringLocked.localizedLine)
                            .themeText(.body)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                        Button("Try again") {
                            holder.retryOpenIfKeychainWasLocked(
                                trigger: "the retry button"
                            )
                        }
                        .buttonStyle(PrimaryButtonStyle())
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
            // Every unlock a person is present for makes the scene active, so
            // this is the only retry trigger a refused keychain read needs.
            if phase == .active {
                holder.retryOpenIfKeychainWasLocked(
                    trigger: "the scene becoming active"
                )
            }
            // The only playback save point on iOS, since core keeps running for
            // background audio; the background task keeps iOS from suspending
            // the process before the save returns.
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
            .foregroundStyle(Theme.danger)
            .padding(ThemeSpace.page)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Owns one UIKit background task and ends it exactly once, whether the save
/// finishes or iOS expires it, because ending it twice is an over-release.
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
