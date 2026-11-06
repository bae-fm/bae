import AppKit
import SwiftUI
import Testing

@testable import bae

@MainActor
@Suite("The Discogs key's field")
struct DiscogsKeyFocusTests {
    /// The key field takes the keyboard as soon as it is in a window, which
    /// SwiftUI gives it only after installing it: Settings opened at the key
    /// lands the cursor in it.
    @Test("the key field takes the keyboard as it shows")
    func theKeyFieldTakesTheKeyboard() async throws {
        let size = NSSize(width: 500, height: 200)
        try await SnapshotTestSupport.withHostedWindow(
            DiscogsSettingsContent(
                draft: .constant(""),
                status: .notConfigured,
                isValidating: false,
                saveError: nil,
                readError: nil,
                onSave: {},
                onRecheck: {},
                onRemove: {},
            )
            .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            let field = try #require(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSTextField }
                    .first {
                        $0.placeholderString
                            == String(localized: "Paste your key here")
                    }
            )
            try await Wait.untilLaidOut(host) {
                field.currentEditor() != nil
                    && field.currentEditor() === window.firstResponder
            }
        }
    }
}
