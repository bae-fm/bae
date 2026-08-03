import BaeKit
import Foundation
import Testing

@testable import bae

@MainActor
@Suite("Casting settings")
struct CastSettingsTests {

    @Test("turning casting on never asks")
    func enablingAppliesDirectly() {
        #expect(
            Cast.toggleAction(enabled: true, castingDeviceName: nil)
                == .apply(true)
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
            Cast.toggleAction(enabled: false, castingDeviceName: nil)
                == .apply(false)
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

    @Test("the cast status drives the picker's active device")
    func statusCarriesTheCastingDevice() {
        let store = CastStore()
        #expect(store.castingDeviceName == nil)

        store.applyStatus(deviceName: "Living Room Speaker")
        #expect(store.castingDeviceName == "Living Room Speaker")

        store.applyStatus(deviceName: nil)
        #expect(store.castingDeviceName == nil)
    }

    @Test("the cast strings ship in every locale")
    func castStringsHaveEveryLocalization() throws {
        let strings = try catalogStrings("bae/Localizable.xcstrings")
        let reference = try locales(of: "Cast", in: strings)
        for key in [
            "Casting",
            "Casting to %@",
            "Disconnect",
            "Enable casting",
            "No Cast devices found",
            "Plays to Cast and AirPlay receivers on your network. While off, bae does not look for devices.",
            "This will stop casting to %@.",
            "Turn Off",
            "Turn off casting?",
        ] {
            #expect(
                try locales(of: key, in: strings) == reference,
                "\(key) is missing locales"
            )
        }
    }

    /// The local-network prompt the system shows before the first browse is
    /// user-visible text like any other, so it carries the same locale set.
    @Test("the Info.plist prompts ship in every locale")
    func usageDescriptionsHaveEveryLocalization() throws {
        let reference = try locales(
            of: "Cast",
            in: catalogStrings("bae/Localizable.xcstrings")
        )
        let plistStrings = try catalogStrings("bae/InfoPlist.xcstrings")
        for key in ["NSCameraUsageDescription", "NSLocalNetworkUsageDescription"]
        {
            #expect(
                try locales(of: key, in: plistStrings) == reference,
                "\(key) is missing locales"
            )
        }
    }

    private func catalogStrings(
        _ relativePath: String
    ) throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appending(path: relativePath)
        let catalog = try #require(
            try JSONSerialization.jsonObject(with: Data(contentsOf: url))
                as? [String: Any]
        )
        return try #require(catalog["strings"] as? [String: Any])
    }

    private func locales(
        of key: String,
        in strings: [String: Any]
    ) throws -> Set<String> {
        let entry = try #require(strings[key] as? [String: Any])
        let localizations = try #require(
            entry["localizations"] as? [String: Any]
        )
        return Set(localizations.keys)
    }
}
