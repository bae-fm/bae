import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Creating an invitation gives the invited person access to storage over
/// the network, so the sheet says so while it runs.
@MainActor
@Suite("Invite person sheet")
struct InvitePersonSheetTests {
    /// Whether the sheet has asked for the invite.
    private actor InviteRecorder {
        var asked = false
        func ask() { asked = true }
    }

    @Test("a running invite says it is creating the invitation")
    func creatingIsNamed() async throws {
        let recorder = InviteRecorder()
        // An invite that never finishes keeps the sheet on its wait.
        let sync = Sync(invitePerson: { _, _, _, _ in
            await recorder.ask()
            try await Task.sleep(for: .seconds(3_600))
            throw CancellationError()
        })
        let size = NSSize(width: 440, height: 520)
        try await SnapshotTestSupport.withHostedWindow(
            InvitePersonSheet(onCreated: { _ in }, onDismiss: {})
                .environment(sync)
                .environment(
                    PreviewData.makeConfigStore(libraryFullWidth: false)
                )
                .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            #expect(
                !(try await drawnLines(host, size: size))
                    .carrying("Creating the invitation")
            )

            let fields = SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTextField }
                .filter(\.isEditable)
            try #require(fields.count == 2, "fields \(fields)")
            for (field, text) in zip(fields, ["Ben", "ben@example.com"]) {
                #expect(HostedInput.focus(field, in: window))
                field.stringValue = text
                field.delegate?.controlTextDidChange?(
                    Notification(
                        name: NSControl.textDidChangeNotification,
                        object: field
                    )
                )
                try await SnapshotTestSupport.settle(host)
            }
            #expect(HostedInput.focus(nil, in: window))
            try await SnapshotTestSupport.settle(host)

            try HostedInput.keyEquivalent(.return, in: host)
            try await Wait.until { await recorder.asked }
            try await SnapshotTestSupport.settle(host)
            let lines = try await drawnLines(host, size: size)
            #expect(lines.carrying("Creating the invitation"), "drew \(lines)")
        }
    }

    private func drawnLines(_ host: NSView, size: NSSize) async throws
        -> [String]
    {
        let png = try await SnapshotTestSupport.capturePNG(host, size: size)
        return try await SnapshotTestSupport.recognizedText(in: png)
            .map(\.text)
    }
}
