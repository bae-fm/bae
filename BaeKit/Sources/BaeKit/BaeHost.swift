import Foundation

/// Builds the process-lifetime `BridgeHost` the app holds for its whole run:
/// the object that carries bae's directory, the OAuth applications, and the
/// runtime that create, restore, join and sign-in run on.
public enum BaeHost {
    /// Build the host around the telemetry sink and bae's directory, and on
    /// iOS the audio session every library's playback asks before it starts.
    /// Call once at startup, right after `BaeDiagnostics.configure`. Building
    /// it opens the keychain every library's keys live in. It fails when the
    /// keychain can't be opened, which on Apple's platforms only a wrong
    /// keychain service name causes, or when the OS refuses the onboarding
    /// runtime's worker threads; either leaves the app unable to restore,
    /// join, sign in, or open a library, so it stops the launch with the
    /// reason.
    public static func make(
        diagnostics: BridgeDiagnostics,
        appDir: BridgeAppDir
    ) -> BridgeHost {
        #if os(iOS)
            make {
                try BridgeHost(
                    diagnostics: diagnostics,
                    appDir: appDir,
                    audioFocus: PlaybackAudioSession()
                )
            }
        #else
            make { try BridgeHost(diagnostics: diagnostics, appDir: appDir) }
        #endif
    }

    /// Build the host with `construct`, such as a debug app's host whose
    /// keychain stays in memory for its UI tests; it fails as `make` does.
    public static func make(
        _ construct: () throws -> BridgeHost
    ) -> BridgeHost {
        do {
            return try construct()
        }
        catch {
            preconditionFailure("The host could not start: \(error)")
        }
    }

    #if DEBUG && os(macOS)
        /// A host over a fresh bae directory in a temporary folder, which
        /// goes when the host does, whose keys stay in an in-memory keychain,
        /// for a preview's attempt, which reads no real library and never the
        /// person's keychain. Only the macOS app's previews use it, and every
        /// debug macOS bridge has the in-memory keychain and the temporary
        /// directory (`build-macos.sh` adds `test-keyring`).
        public static func preview() -> BridgeHost {
            make {
                try BridgeHost.withTestKeyring(
                    diagnostics: BaeDiagnostics.disabled(),
                    appDir: BridgeAppDir.temporary()
                )
            }
        }
    #endif
}
