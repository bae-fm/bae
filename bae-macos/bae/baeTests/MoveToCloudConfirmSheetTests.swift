import AppKit
import SwiftUI
import XCTest

@testable import bae

@MainActor
final class MoveToCloudConfirmSheetTests: XCTestCase {
    func testPinChoiceUsesImportPinPreference() async throws {
        let defaults = UserDefaults.standard
        let key = StoragePinPreference.userDefaultsKey
        let previous = defaults.object(forKey: key)
        defaults.set(false, forKey: key)
        defer {
            if let previous {
                defaults.set(previous, forKey: key)
            }
            else {
                defaults.removeObject(forKey: key)
            }
        }

        let size = NSSize(width: 420, height: 220)
        try await SnapshotTestSupport.withHostedWindow(
            MoveToCloudConfirmSheet(onConfirm: { _ in }, onCancel: {})
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in

            try await SnapshotTestSupport.settle(host)

            let controlFrames = host.subviews
                .filter { $0.nextKeyView != nil || $0.previousKeyView != nil }
                .map { $0.convert($0.bounds, to: host) }
            for frame in controlFrames where !defaults.bool(forKey: key) {
                try HostedInput.click(at: frame.center, in: host)
                try await SnapshotTestSupport.settle(host)
            }
            XCTAssertTrue(defaults.bool(forKey: key))
        }
    }
}

extension NSRect {
    fileprivate var center: NSPoint {
        NSPoint(x: midX, y: midY)
    }
}
