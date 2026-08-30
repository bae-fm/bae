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

        await SnapshotTestSupport.settle(host)

        let controlFrames = host.subviews
            .filter { $0.nextKeyView != nil || $0.previousKeyView != nil }
            .map { $0.convert($0.bounds, to: host) }
        for frame in controlFrames where !defaults.bool(forKey: key) {
            try click(at: frame.center, in: host, window: window)
            await SnapshotTestSupport.settle(host)
        }
        XCTAssertTrue(defaults.bool(forKey: key))
        withExtendedLifetime(window) {}
    }

    private func click(
        at point: NSPoint,
        in host: NSView,
        window: NSWindow
    ) throws {
        let windowPoint = host.convert(point, to: nil)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let event = try XCTUnwrap(
                NSEvent.mouseEvent(
                    with: type,
                    location: windowPoint,
                    modifierFlags: [],
                    timestamp: ProcessInfo.processInfo.systemUptime,
                    windowNumber: window.windowNumber,
                    context: nil,
                    eventNumber: 0,
                    clickCount: 1,
                    pressure: type == .leftMouseDown ? 1 : 0
                )
            )
            window.sendEvent(event)
        }
    }
}

extension NSRect {
    fileprivate var center: NSPoint {
        NSPoint(x: midX, y: midY)
    }
}
