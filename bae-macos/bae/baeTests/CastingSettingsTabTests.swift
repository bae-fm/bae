import BaeKit
import Foundation
import Testing

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
            "Turn Off",
        ] {
            #expect(try locales(key) == reference, "\(key) is missing locales")
        }
    }
}
