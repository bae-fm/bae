import BaeKit
import Foundation
import SwiftUI

@testable import bae

/// The named UI scenes captured for the cross-platform screenshot gallery.
/// Each builder composes production views over `PreviewData` fixtures.
@MainActor
enum ShotScenes {
    /// The welcome window entered at `initialMode`, in its fixed 690x600
    /// chrome, over `setup` and the `LibraryRemovals` fixtures. Every flow
    /// can go back to the chooser, which reads both lists from `setup`.
    static func welcome(
        initialMode: WelcomeView.Mode,
        setup: LibrarySetup,
        removals: LibraryRemovals = PreviewData.welcomeRemovals(),
        loadError: LoadError? = nil
    ) -> some View {
        MacWelcomeWindow {
            WelcomeView(
                onLibraryReady: { _, _ in },
                onOpenLibrary: { _ in },
                onFinishReplacement: { _, _ in },
                onResetFromCloud: { _ in },
                loadError: loadError,
                initialMode: initialMode,
                canDeleteActiveLibrary: true
            )
        }
        .environment(setup)
        .environment(removals)
    }

    /// What the details popover of `library` holds, its status read
    /// through `readStanding`.
    static func welcomeLibraryDetails(
        _ library: BridgeLibrary,
        readStanding:
            @escaping @Sendable (String) async throws
            -> BridgeLibraryStanding
    ) -> some View {
        WelcomeLibraryDetails(
            library: library,
            readStanding: readStanding
        )
        .windowBackground()
    }

    /// The screen that offers resetting this device, in the welcome
    /// window's chrome, saying `error`, or with none, the reset's own line.
    static func deviceReset(error: LoadError?) -> some View {
        MacWelcomeWindow {
            DeviceResetView(
                prompt: .offered,
                error: error,
                onReset: {},
                onCancel: {}
            )
        }
    }
}
