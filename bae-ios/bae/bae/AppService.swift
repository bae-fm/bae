import BaeKit
import Foundation

/// iOS `AppService`: the shared `BaeKit.AppService` base with the iOS wiring.
/// The desktop import/export/preview layer the macOS subclass carries has no
/// iOS counterpart; what it does add is the system service browser that stands
/// in for the network browsing bae is not allowed to do here. The rest is the
/// platform's `init` (no `mediaControlService`/`uiStore` to inject) and `wireUp`
/// (audio-session remote commands instead of the preview session).
/// Named to shadow the base so `@Environment(AppService.self)` reads resolve to
/// it.
@MainActor
final class AppService: BaeKit.AppService {
    /// The system Bonjour browser that feeds core's device list on iOS, where
    /// bae may not browse the network itself.
    let renderers: RendererBrowser

    init(
        appHandle: AppHandle,
        diagnostics: BridgeDiagnostics,
        config: BridgeConfig,
        initialOutbox: BridgeOutboxSnapshot
    ) {
        renderers = RendererBrowser(handle: appHandle)
        super
            .init(
                appHandle: appHandle,
                mediaControlService: MediaControlService(),
                diagnostics: diagnostics,
                config: config,
                initialOutbox: initialOutbox
            )
    }

    /// Register the common projections, subscribe to the live event stream, and
    /// set up the lock-screen / Control-Center remote commands. Called once
    /// after construction. Errors route through the base's default `showError`
    /// (the `ConfigStore` banner).
    func wireUp() {
        registerCommonProjections()
        appHandle.subscribeUiEvents(
            callback: UiEventPump(
                sink: UiEventDispatcher.makeSink(
                    appService: self,
                    onUnhandled: DesktopUiEvents.ignore
                )
            )
        )
        mediaControlService.setupRemoteCommands(
            playback: playback,
            playbackStore: playbackStore
        )
    }
}
