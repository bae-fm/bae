import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing
import os

@testable import bae

@MainActor
@Suite("Play on in the Playback settings")
struct PlaybackSettingsTabTests {
    @Test("turning Play on other devices on never asks")
    func enablingAppliesDirectly() {
        #expect(
            Cast.toggleAction(
                enabled: true,
                castingDeviceName: nil
            ) == .apply(true)
        )
        #expect(
            Cast.toggleAction(
                enabled: true,
                castingDeviceName: "Living Room Speaker"
            ) == .apply(true)
        )
    }

    @Test(
        "turning Play on other devices off with nothing playing applies directly"
    )
    func disablingIdleAppliesDirectly() {
        #expect(
            Cast.toggleAction(
                enabled: false,
                castingDeviceName: nil
            ) == .apply(false)
        )
    }

    @Test(
        "turning Play on other devices off while playing asks, naming the receiver"
    )
    func disablingWhileCastingConfirms() {
        #expect(
            Cast.toggleAction(
                enabled: false,
                castingDeviceName: "Living Room Speaker"
            ) == .confirmDisconnect(device: "Living Room Speaker")
        )
    }

    @Test("the pane browses from its window opening until it closes")
    func browsesWhileTheWindowIsOpen() async throws {
        let asks = OSAllocatedUnfairLock(initialState: [String]())
        let cast = Cast(
            startDiscovery: { asker in
                asks.withLock { $0.append("start \(asker)") }
            },
            stopDiscovery: { asker in
                asks.withLock { $0.append("stop \(asker)") }
            }
        )
        let owner = WindowOwner()
        try autoreleasepool {
            let controller = HostingWindowController(
                content: PlaybackSettingsTab()
                    .environment(cast)
                    .environment(Playback.stub())
                    .environment(CastStart.stub())
                    .environment(CastStore())
                    .environment(
                        PreviewData.makeConfigStore(
                            libraryFullWidth: false,
                            castEnabled: true
                        )
                    )
                    .environment(UiStore()),
                contentSize: CGSize(width: 500, height: 300),
                sizing: [],
                bridging: [],
                onClose: { owner.controller = nil }
            )
            owner.controller = controller
            let window = try #require(controller.window)
            // Past every display, so nothing shows on the screen of the
            // person running the suite.
            let right = NSScreen.screens.map(\.frame.maxX).max() ?? 0
            window.setFrameOrigin(NSPoint(x: right + 1_000, y: 0))
            window.animationBehavior = .none
            window.orderFront(nil)
        }
        try await Wait.until {
            asks.withLock { $0 == ["start settingsPage"] }
        }

        autoreleasepool {
            owner.controller?.window?.close()
        }
        try await Wait.until {
            asks.withLock { $0 == ["start settingsPage", "stop settingsPage"] }
        }
    }

    @Test("the Play on strings ship in every locale")
    func playOnStringsHaveEveryLocalization() throws {
        let strings = try StringCatalog.strings()

        func locales(_ key: String) throws -> Set<String> {
            let entry = try #require(strings[key] as? [String: Any])
            let localizations = try #require(
                entry["localizations"] as? [String: Any]
            )
            return Set(localizations.keys)
        }

        let reference = try locales("Play on…")
        for key in [
            "Playing on %@",
            "No receivers found",
            "Play on other devices",
            "Scan for Cast, AirPlay, and UPnP receivers on your network.",
            "Stop playing on %@?",
            "Could not play on %@",
            "Could not play on the receiver",
            "Turn off",
            "Receivers on your network",
            "Looking for receivers…",
        ] {
            #expect(try locales(key) == reference, "\(key) is missing locales")
        }
    }
}
