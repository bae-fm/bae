import BaeKit

/// The transient events the shared `UiEventDispatcher` declines that macOS
/// still handles itself: per-track loudness-measurement progress during an
/// import, the import queue's identify progress, and a watched folder whose
/// scan failed. The owner traps if a shared-handled event reaches this tail.
enum DesktopUiEvents {
    @MainActor
    static func apply(_ event: BridgeUiEvent, appService: AppService) {
        appService.applyDesktopUIEvent(event)
    }
}
