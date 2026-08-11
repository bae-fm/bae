import BaeKit
import Foundation
import SwiftUI

/// iOS `AppService`: the shared `BaeKit.AppService` base with the iOS wiring.
/// The desktop import/export/preview layer the macOS subclass carries has no
/// iOS counterpart; what it does add is the system service browser that stands
/// in for the network browsing bae is not allowed to do here. The rest is the
/// platform's `init` (no `mediaControlService`/`uiStore` to inject) and `wireUp`
/// (audio-session remote commands instead of the preview session).
/// Named to shadow the base so `@Environment(AppService.self)` reads resolve to
/// it.
@MainActor
final class AppService: BaeKit.AppService, @unchecked Sendable {
    /// The system Bonjour browser that feeds core's device list on iOS, where
    /// bae may not browse the network itself.
    private let renderers: RendererBrowser

    init(
        appHandle: AppHandle,
        diagnostics: BridgeDiagnostics,
        config: BridgeConfig,
        initialOutbox: BridgeOutboxSnapshot
    ) {
        let components = AppServiceComponents(
            playbackStore: PlaybackStore(),
            configStore: ConfigStore(
                config: Config(bridge: config),
                syncReady: appHandle.isSyncReady()
            ),
            libraryStore: LibraryStore(),
            downloadStore: DownloadStore(
                snapshot: appHandle.getDownloadSnapshot()
            ),
            castStore: CastStore(),
            outboxStore: OutboxStore(snapshot: initialOutbox),
            projectionRegistry: ProjectionRegistry(),
            library: Library(handle: appHandle),
            playback: Playback(handle: appHandle),
            queue: Queue(handle: appHandle),
            mediaPaths: MediaPaths(handle: appHandle),
            imageStore: ImageStore(handle: appHandle),
            sync: Sync(handle: appHandle),
            downloads: Downloads(handle: appHandle),
            cast: Cast(handle: appHandle)
        )
        renderers = RendererBrowser(handle: appHandle)
        super
            .init(
                appHandle: appHandle,
                mediaControlService: MediaControlService(),
                diagnostics: diagnostics,
                components: components
            )
    }

    /// Register the common projections, subscribe to the live event stream, and
    /// set up the lock-screen / Control-Center remote commands. Called once
    /// after construction. Errors route through the base's default `showError`
    /// (the `ConfigStore` banner).
    func wireUp() {
        registerCommonProjections()
        subscribeUIEvents(onUnhandled: DesktopUiEvents.ignore)
        setupRemoteCommands()
    }

    func installEnvironment<Content: View>(_ content: Content) -> some View {
        installSharedEnvironment(content)
            .environment(renderers)
    }
}
