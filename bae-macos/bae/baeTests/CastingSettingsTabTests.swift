import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing
import os

@testable import bae

@MainActor
@Suite("Casting settings")
struct CastingSettingsTabTests {

    @Test("turning casting on never asks")
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

    @Test("turning casting off with nothing casting applies directly")
    func disablingIdleAppliesDirectly() {
        #expect(
            Cast.toggleAction(
                enabled: false,
                castingDeviceName: nil
            ) == .apply(false)
        )
    }

    @Test("turning casting off mid-session asks, naming the device")
    func disablingWhileCastingConfirms() {
        #expect(
            Cast.toggleAction(
                enabled: false,
                castingDeviceName: "Living Room Speaker"
            ) == .confirmDisconnect(device: "Living Room Speaker")
        )
    }

    @Test("the page browses from its window opening until it closes")
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
                content: CastingSettingsTab()
                    .environment(cast)
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

    @Test("the casting settings strings ship in every locale")
    func castingStringsHaveEveryLocalization() throws {
        let catalogURL = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appending(path: "bae/Localizable.xcstrings")
        let catalog = try #require(
            try JSONSerialization.jsonObject(
                with: Data(contentsOf: catalogURL)
            ) as? [String: Any]
        )
        let strings = try #require(catalog["strings"] as? [String: Any])

        func locales(_ key: String) throws -> Set<String> {
            let entry = try #require(strings[key] as? [String: Any])
            let localizations = try #require(
                entry["localizations"] as? [String: Any]
            )
            return Set(localizations.keys)
        }

        let reference = try locales("Cast")
        for key in [
            "Casting",
            "Enable casting",
            "Plays to Cast, AirPlay, and UPnP receivers on your network. While off, bae does not look for devices.",
            "Turn off casting?",
            "This will stop casting to %@.",
            "Turn off",
            "Receivers on your network",
            "Looking for receivers…",
            "Receivers show up here while this page is open.",
        ] {
            #expect(try locales(key) == reference, "\(key) is missing locales")
        }
    }
}
