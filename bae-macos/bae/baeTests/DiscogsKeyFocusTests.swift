import AppKit
import Observation
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
                isEnabled: true,
                isValidating: false,
                wasRejected: false,
                saveError: nil,
                readError: nil,
                onSave: {},
                onRecheck: {},
            )
            .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            let field = try #require(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSTextField }
                    .first {
                        $0.placeholderString
                            == String(localized: "Your free Discogs key")
                    }
            )
            try await Wait.untilLaidOut(host) {
                field.currentEditor() != nil
                    && field.currentEditor() === window.firstResponder
            }
        }
    }

    /// Switched off, the field is greyed and the keyboard stays where it was;
    /// as the switch turns it on, the field takes the keyboard.
    @Test("the key field takes the keyboard as the switch turns it on")
    func theKeyFieldTakesTheKeyboardWhenTheSwitchTurnsOn() async throws {
        let size = NSSize(width: 500, height: 200)
        let search = SearchSwitch()
        try await SnapshotTestSupport.withHostedWindow(
            SwitchedKeyRows(search: search)
                .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            let field = try #require(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSTextField }
                    .first {
                        $0.placeholderString
                            == String(localized: "Your free Discogs key")
                    }
            )
            #expect(!field.isEnabled)
            #expect(field.currentEditor() == nil)

            search.isOn = true
            try await Wait.untilLaidOut(host) {
                field.isEnabled && field.currentEditor() != nil
                    && field.currentEditor() === window.firstResponder
            }
        }
    }
}

/// The Search Discogs switch's state, which the key's rows follow.
@MainActor
@Observable
private final class SearchSwitch {
    var isOn = false
}

private struct SwitchedKeyRows: View {
    let search: SearchSwitch

    var body: some View {
        DiscogsSettingsContent(
            draft: .constant(""),
            status: .notConfigured,
            isEnabled: search.isOn,
            isValidating: false,
            wasRejected: false,
            saveError: nil,
            readError: nil,
            onSave: {},
            onRecheck: {},
        )
    }
}
