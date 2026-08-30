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
        let (window, host) = SnapshotTestSupport.hostInWindow(
            MoveToCloudConfirmSheet(onConfirm: { _ in }, onCancel: {})
                .frame(width: size.width, height: size.height),
            size: size
        )

        host.layoutSubtreeIfNeeded()
        try await Task.sleep(for: .milliseconds(250))
        host.layoutSubtreeIfNeeded()

        let toggle = try XCTUnwrap(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSButton }
                .first { $0.title == String(localized: "Pinned") }
        )
        XCTAssertEqual(toggle.state, .off)

        toggle.performClick(nil)
        await Task.yield()
        XCTAssertTrue(defaults.bool(forKey: key))
        withExtendedLifetime(window) {}
    }
}
