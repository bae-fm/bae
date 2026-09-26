import AppKit
import BaeKit
import SwiftUI
import XCTest

@testable import bae

/// The sheet's pin choice is the stored one an import makes: it opens on it,
/// and moving it writes it back through core.
@MainActor
final class MoveToCloudConfirmSheetTests: XCTestCase {
    func testPinChoiceWritesTheStoredImportChoice() async throws {
        var writes: [Bool] = []
        let importer = Importer(setImportPinned: { writes.append($0) })
        let size = NSSize(width: 420, height: 220)
        try await SnapshotTestSupport.withHostedWindow(
            MoveToCloudConfirmSheet(onConfirm: { _ in }, onCancel: {})
                .frame(width: size.width, height: size.height)
                .environment(PreviewData.configStore())
                .environment(importer)
                .environment(UiStore()),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            // The sheet's buttons do nothing here, so clicking every control
            // until one writes reaches the pin switch whatever it renders as.
            let controlFrames = host.subviews
                .filter { $0.nextKeyView != nil || $0.previousKeyView != nil }
                .map { $0.convert($0.bounds, to: host) }
            for frame in controlFrames where writes.isEmpty {
                try HostedInput.click(
                    at: NSPoint(x: frame.midX, y: frame.midY),
                    in: host
                )
                try await SnapshotTestSupport.settle(host)
            }

            // The preview library's stored choice is pinned, so moving the
            // switch stores the other answer.
            XCTAssertEqual(writes, [false])
        }
    }
}
