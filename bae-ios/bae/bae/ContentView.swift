import SwiftUI

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
    private var holder = AppSessionHolder()
    @State
    private var started = false
    @Environment(\.scenePhase)
    private var scenePhase

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
                        .environment(service.projectionRegistry)
                        .environment(service.mediaPaths)
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
            // save point on iOS.
            if phase == .background {
                holder.appService?.appHandle.savePlaybackState()
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
