import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct TriageTabBarTests {
    @Test(
        "Import tabs accept clicks across their full segment",
        arguments: [320.0, 600.0, 900.0]
    )
    func fullHitArea(width: Double) async throws {
        let selection = Selection()
        let size = NSSize(width: width, height: 40)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            TriageTabBar(
                activeTab: Binding(
                    get: { selection.tab },
                    set: { selection.tab = $0 }
                ),
                counts: BridgeTriageTabCounts(
                    pending: 170,
                    done: 32,
                    skipped: 19
                )
            )
            .frame(width: size.width, height: size.height),
            size: size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)
        let tabs: [BridgeTriageTab] = [.pending, .done, .skipped]
        let segmentWidth = (width - 8) / 3
        for (index, tab) in tabs.enumerated() {
            // Exercise blank padding on every side of the label and badge.
            for point in [
                NSPoint(x: 4, y: 20),
                NSPoint(x: segmentWidth - 4, y: 20),
                NSPoint(x: segmentWidth / 2, y: 9),
                NSPoint(x: segmentWidth / 2, y: 31),
            ] {
                selection.tab = tab == .pending ? .done : .pending
                await SnapshotTestSupport.settle(host)
                try click(
                    window,
                    at: NSPoint(
                        x: Double(index) * (segmentWidth + 4) + point.x,
                        y: point.y
                    )
                )
                await SnapshotTestSupport.settle(host)
                #expect(selection.tab == tab)
            }
        }
    }

    @Observable
    fileprivate final class Selection {
        var tab: BridgeTriageTab = .pending
    }

    private func click(_ window: NSWindow, at point: NSPoint) throws {
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            window.sendEvent(
                try #require(
                    NSEvent.mouseEvent(
                        with: type,
                        location: point,
                        modifierFlags: [],
                        timestamp: ProcessInfo.processInfo.systemUptime,
                        windowNumber: window.windowNumber,
                        context: nil,
                        eventNumber: 0,
                        clickCount: 1,
                        pressure: type == .leftMouseDown ? 1 : 0
                    )
                )
            )
        }
    }
}
