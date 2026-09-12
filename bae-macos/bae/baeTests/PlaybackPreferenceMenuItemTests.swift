import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing

@testable import bae

/// The Playback menu's preference items. Each shows the stored value and,
/// clicked, writes the opposite of what it showed — never a flip of whatever
/// the store holds by the time the click lands.
@MainActor
@Suite("A playback preference menu item")
struct PlaybackPreferenceMenuItemTests {
    private static let size = NSSize(width: 260, height: 32)

    @Test("the item shows the stored value")
    func theItemShowsTheStoredValue() async throws {
        let on = try await pixels(isOn: true)
        let off = try await pixels(isOn: false)

        #expect(
            on != off,
            "the checkmark is the item's only mark of the stored value"
        )
    }

    @Test("a click writes the opposite of what the item shows")
    func aClickWritesTheOppositeOfWhatTheItemShows() async throws {
        for isOn in [true, false] {
            let recorder = PreferenceWriteRecorder()
            let (window, host) = SnapshotTestSupport.hostInWindow(
                item(isOn: isOn) { recorder.writes.append($0) },
                size: Self.size
            )
            defer {
                window.contentView = nil
                window.orderOut(nil)
            }
            await SnapshotTestSupport.settle(host)

            try click(at: host.bounds.center, in: host, window: window)
            await SnapshotTestSupport.settle(host)

            #expect(recorder.writes == [!isOn])
        }
    }

    private func pixels(isOn: Bool) async throws -> Data {
        let (window, host) = SnapshotTestSupport.hostInWindow(
            item(isOn: isOn) { _ in },
            size: Self.size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)
        return try await SnapshotTestSupport.capturePNG(host, size: Self.size)
    }

    private func item(
        isOn: Bool,
        setEnabled: @escaping (Bool) -> Void
    ) -> some View {
        PlaybackPreferenceMenuItem(
            title: "Pause between sides and discs",
            isOn: isOn,
            setEnabled: setEnabled
        )
        .frame(width: Self.size.width, height: Self.size.height)
    }

    private func click(
        at point: NSPoint,
        in host: NSView,
        window: NSWindow
    ) throws {
        let windowPoint = host.convert(point, to: nil)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let event = try #require(
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

@MainActor
private final class PreferenceWriteRecorder {
    var writes: [Bool] = []
}

extension NSRect {
    fileprivate var center: NSPoint {
        NSPoint(x: midX, y: midY)
    }
}
