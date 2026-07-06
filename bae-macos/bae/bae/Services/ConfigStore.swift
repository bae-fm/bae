import SwiftUI

/// Mirror of core's library configuration. The reducer is the sole writer:
/// views read the current `Config` (including sync settings) and invoke bridge
/// methods to change them; the resulting `configChanged` event flows back here.
@Observable
class ConfigStore {
    var config: Config
    /// Whether the sync loop is running right now. Runtime status, not
    /// configuration: it rides the `configChanged` event alongside `config`
    /// but lands here, not on the `Config` mirror, since it changes
    /// independently of any persisted setting. Settings/pairing gate on this.
    var syncReady: Bool
    /// Sync loop's latest error, or nil when sync is healthy. Set/cleared by
    /// the reducer from the `syncError` UI event. The Library settings tab
    /// surfaces this as a reconnect banner (generic line + copyable detail).
    var syncError: DisplayError?
    /// Wall-clock time of the most recent successful sync cycle, or nil before
    /// the first cycle completes. The reducer parses the RFC 3339 string from
    /// the `syncTimeChanged` UI event into a `Date` here, so the sidebar reads a
    /// typed value and only has to format it relative to now.
    var lastSyncTime: Date?
    /// Whether the sync loop is currently mid-cycle. The sidebar reads this
    /// to overlay a spinner on the active library row from "Sync Now"
    /// through to the cycle finishing.
    var syncing: Bool = false

    #if os(iOS)
        /// Latest surfaced error from core's `error` event, or nil when
        /// cleared. iOS routes app errors here for the library banner; macOS
        /// surfaces them through its global alert (`UiStore`) instead. Typed:
        /// the reducer builds a `DisplayError` from the bridge's `BridgeError`
        /// (or a `BridgePlaybackErrorReason`) so the banner renders the
        /// localized line for the device locale, never a pre-formatted string.
        var lastError: DisplayError?

        /// Surface a UI-originated error — prose the UI already localized (a
        /// caught Swift error, a keychain write failure). Core errors crossing
        /// the bridge use the typed overload.
        func showError(_ message: String) {
            lastError = DisplayError(line: message)
        }

        /// Surface a typed core failure — renders its generic per-category line
        /// for the device locale, with the opaque detail carried along.
        func showError(_ error: DisplayError) {
            lastError = error
        }

        func clearError() {
            lastError = nil
        }
    #endif

    init(config: Config, syncReady: Bool) {
        self.config = config
        self.syncReady = syncReady
    }

    func applyConfigSnapshot(_ config: BridgeConfig, syncReady: Bool) {
        self.config = Config(bridge: config)
        self.syncReady = syncReady
    }

    func applySyncStatusSnapshot(_ snapshot: BridgeSyncStatusSnapshot) {
        syncError = snapshot.error.map(DisplayError.init)
        lastSyncTime = snapshot.lastSyncTime.map {
            Date(timeIntervalSince1970: TimeInterval($0) / 1000)
        }
        syncing = snapshot.syncing
        syncReady = snapshot.syncReady
    }
}
