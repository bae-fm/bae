import SwiftUI

/// Mirror of core's library configuration. The config value stream is its only
/// writer.
@Observable
public class ConfigStore {
    public var config: Config

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

        /// Surface a caught error. An error core says has no line — a
        /// cancellation — is dropped rather than shown as an empty banner.
        public func showError(_ error: any Error) {
            guard let displayed = DisplayError(error) else { return }
            lastError = displayed
        }

        public func clearError() {
            lastError = nil
        }
    #endif

    public init(config: Config) {
        self.config = config
    }

    public func applyConfigSnapshot(_ config: BridgeConfig) {
        self.config = Config(bridge: config)
    }

}
