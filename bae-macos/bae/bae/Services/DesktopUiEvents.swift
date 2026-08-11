import BaeKit

/// The events the shared `UiEventDispatcher` declines that macOS still handles
/// itself: import preview playback, per-track loudness-measurement progress
/// during an import, and the import queue's identify progress. The owner traps
/// if a shared-handled event reaches this tail.
enum DesktopUiEvents {
    @MainActor
    static func apply(_ event: BridgeUiEvent, appService: AppService) {
        appService.applyDesktopUIEvent(event)
    }
}
