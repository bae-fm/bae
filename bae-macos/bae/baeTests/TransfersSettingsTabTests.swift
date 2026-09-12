import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing

@testable import bae

/// The two transfer-concurrency controls, and that each writes the direction it
/// names. They are separate settings: how much this machine pushes at once says
/// nothing about how much it pulls.
@MainActor
@Suite("The transfer settings")
struct TransfersSettingsTabTests {
    private static let size = NSSize(width: 520, height: 300)

    @Test("the tab draws one picker per direction")
    func theTabDrawsOnePickerPerDirection() async throws {
        let recorder = TransferConcurrencyRecorder()
        let (window, host) = SnapshotTestSupport.hostInWindow(
            tab(recorder: recorder),
            size: Self.size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)

        #expect(pickers(in: host).count == 2)
    }

    @Test("each picker writes its own direction")
    func eachPickerWritesItsOwnDirection() async throws {
        let recorder = TransferConcurrencyRecorder()
        let (window, host) = SnapshotTestSupport.hostInWindow(
            tab(recorder: recorder),
            size: Self.size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)

        // Uploads first, downloads second, in the order the tab lists them.
        // Segment n stands for n + 1 simultaneous transfers.
        let controls = pickers(in: host)
        try #require(controls.count == 2)
        select(segment: 4, in: controls[0])
        await SnapshotTestSupport.settle(host)
        select(segment: 0, in: controls[1])
        await SnapshotTestSupport.settle(host)

        #expect(recorder.uploadWrites == [5])
        #expect(recorder.downloadWrites == [1])
    }

    private func tab(recorder: TransferConcurrencyRecorder) -> some View {
        TransfersSettingsTab()
            .environment(PreviewData.configStore())
            .environment(recorder.downloads)
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
private final class TransferConcurrencyRecorder {
    var uploadWrites: [UInt32] = []
    var downloadWrites: [UInt32] = []

    /// Both setters are plain `@Sendable`; the picker calls them from the main
    /// actor, which is where this recorder's state lives.
    var sync: Sync {
        Sync(
            setMaxConcurrentUploads: { [self] n in
                MainActor.assumeIsolated { uploadWrites.append(n) }
            }
        )
    }

    var downloads: Downloads {
        Downloads(
            setMaxConcurrentDownloads: { [self] n in
                MainActor.assumeIsolated { downloadWrites.append(n) }
            }
        )
    }
}
