import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing

@testable import bae

/// The two pause switches and the two transfer-concurrency controls, and that
/// each writes the direction it names. They are separate settings: how much
/// this machine pushes at once says nothing about how much it pulls.
@MainActor
@Suite("The transfer settings")
struct TransfersSettingsTabTests {
    private static let size = NSSize(width: 520, height: 300)

    @Test("the tab draws one picker per direction")
    func theTabDrawsOnePickerPerDirection() async throws {
        let recorder = TransferConcurrencyRecorder()
        try await SnapshotTestSupport.withHostedWindow(
            tab(recorder: recorder),
            size: Self.size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            #expect(pickers(in: host).count == 2)
        }
    }

    @Test("each picker writes its own direction")
    func eachPickerWritesItsOwnDirection() async throws {
        let recorder = TransferConcurrencyRecorder()
        try await SnapshotTestSupport.withHostedWindow(
            tab(recorder: recorder),
            size: Self.size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            // Uploads first, downloads second, in the order the tab lists them.
            // Segment n stands for n + 1 simultaneous transfers.
            let controls = pickers(in: host)
            try #require(controls.count == 2)
            select(segment: 4, in: controls[0])
            try await Wait.until { !recorder.uploadWrites.isEmpty }
            select(segment: 0, in: controls[1])
            try await Wait.until { !recorder.downloadWrites.isEmpty }

            #expect(recorder.uploadWrites == [5])
            #expect(recorder.downloadWrites == [1])
        }
    }

    @Test("each pause switch pauses its own direction")
    func eachPauseSwitchPausesItsOwnDirection() async throws {
        let recorder = TransferConcurrencyRecorder()
        try await SnapshotTestSupport.withHostedWindow(
            tab(recorder: recorder),
            size: Self.size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            // Uploads first, downloads second, in the order the tab lists them.
            let switches = SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSSwitch }
            try #require(switches.count == 2)
            switches[0].performClick(nil)
            try await Wait.until { !recorder.uploadPauses.isEmpty }
            switches[1].performClick(nil)
            try await Wait.until { !recorder.downloadPauses.isEmpty }

            #expect(recorder.uploadPauses == [true])
            #expect(recorder.downloadPauses == [true])
        }
    }

    @Test("what is paused names its arrow and its spoken label")
    func whatIsPausedNamesItsArrow() {
        #expect(PausedTransfers(uploads: false, downloads: false) == nil)
        #expect(
            PausedTransfers(uploads: true, downloads: false)?.arrow
                == "\u{2191}"
        )
        #expect(
            PausedTransfers(uploads: false, downloads: true)?.arrow
                == "\u{2193}"
        )
        #expect(
            PausedTransfers(uploads: true, downloads: true)?.arrow == "\u{21C5}"
        )
    }

    private func tab(recorder: TransferConcurrencyRecorder) -> some View {
        TransfersSettingsTab()
            .environment(PreviewData.configStore())
            .environment(recorder.downloads)
            .environment(DownloadStore())
            .environment(OutboxStore(uploadsPaused: false))
            .environment(recorder.sync)
            .environment(UiStore())
    }

    private func pickers(in host: NSView) -> [NSSegmentedControl] {
        SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSSegmentedControl }
    }

    private func select(segment: Int, in control: NSSegmentedControl) {
        control.selectedSegment = segment
        if let action = control.action {
            NSApp.sendAction(action, to: control.target, from: control)
        }
    }
}

@MainActor
@Observable
private final class TransferConcurrencyRecorder {
    var uploadWrites: [UInt32] = []
    var downloadWrites: [UInt32] = []
    var uploadPauses: [Bool] = []
    var downloadPauses: [Bool] = []

    /// Both setters are plain `@Sendable` async closures; each hops to the
    /// main actor, where this recorder's state lives.
    var sync: Sync {
        Sync(
            setSyncPaused: { [self] paused in
                await MainActor.run { uploadPauses.append(paused) }
            },
            setMaxConcurrentUploads: { [self] n in
                await MainActor.run { uploadWrites.append(n) }
            }
        )
    }

    var downloads: Downloads {
        Downloads(
            setDownloadsPaused: { [self] paused in
                await MainActor.run { downloadPauses.append(paused) }
            },
            setMaxConcurrentDownloads: { [self] n in
                await MainActor.run { downloadWrites.append(n) }
            }
        )
    }
}
