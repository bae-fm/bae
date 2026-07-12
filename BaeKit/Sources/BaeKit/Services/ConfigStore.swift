import SwiftUI

/// Mirror of core's library configuration. The config and sync-status
/// projections are the sole writers: views read the current `Config`
/// (including sync settings) and invoke bridge methods to change them; the
/// resulting `.config` / `.syncStatus` invalidation refreshes this store
/// through the projection registry.
@Observable
public class ConfigStore {
    public var config: Config
    /// Whether the sync loop is running right now. Runtime status, not
    /// configuration: it rides the same `.config` invalidation as `config`
    /// but lands here, not on the `Config` mirror, since it changes
    /// independently of any persisted setting. Settings/pairing gate on this.
    public var syncReady: Bool
    /// Sync loop's latest error, or nil when sync is healthy. Set/cleared by
    /// the sync-status projection from `getSyncStatus()`. The Library settings
    /// tab surfaces this as a reconnect banner (generic line + copyable
    /// detail).
    public var syncError: DisplayError?

    #if os(iOS)
        /// Latest surfaced error from core's `error` event, or nil when
        /// cleared. iOS routes app errors here for the library banner; macOS
        /// surfaces them through its global alert (`UiStore`) instead. Typed:
        /// the shared event dispatcher builds a `DisplayError` from the
        /// bridge's `BridgeError` (or a `BridgePlaybackErrorReason`) so the
        /// banner renders the localized line for the device locale, never a
        /// pre-formatted string.
        public var lastError: DisplayError?

        /// Surface a UI-originated error — prose the UI already localized (a
        /// caught Swift error, a keychain write failure). Core errors crossing
        /// the bridge use the typed overload.
        public func showError(_ message: String) {
            lastError = DisplayError(line: message)
        }

        /// Surface a typed core failure — renders its generic per-category line
        /// for the device locale, with the opaque detail carried along.
        public func showError(_ error: DisplayError) {
            lastError = error
        }

        public func clearError() {
            lastError = nil
        }
    #endif

    public init(config: Config, syncReady: Bool) {
        self.config = config
        self.syncReady = syncReady
    }

    public func applyConfigSnapshot(_ config: BridgeConfig, syncReady: Bool) {
        self.config = Config(bridge: config)
        self.syncReady = syncReady
    }

    public func applySyncStatusSnapshot(_ snapshot: BridgeSyncStatusSnapshot) {
        syncError = snapshot.error.map(DisplayError.init)
        syncReady = snapshot.syncReady
    }
}
