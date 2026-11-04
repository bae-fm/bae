import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing

@testable import bae

/// The per-source switches in Settings › Import, and what the Find online pane
/// says when they leave nothing to ask.
@MainActor
@Suite("The metadata source switches")
struct FindOnlineSourceSwitchTests {

    /// The settings tab's checkboxes write through an Importer call that
    /// passes the source and the value it is being set to straight through.
    /// Absolute, not a flip, so two windows disagreeing cannot leave the
    /// setting inverted.
    @Test("the write carries the source and the value it is being set to")
    func theWriteCarriesTheValueItIsSetTo() async throws {
        let recorder = SourceSwitchRecorder()
        let importer = recorder.importer

        try await importer.setMetadataSourceEnabled(.discogs, false)
        try await importer.setMetadataSourceEnabled(.musicBrainz, true)

        #expect(recorder.writes.map(\.source) == [.discogs, .musicBrainz])
        #expect(recorder.writes.map(\.enabled) == [false, true])
    }

    /// A refusal from core reaches the surface, which is what puts the checkbox
    /// back where it was and shows the reason.
    @Test("a refused write is thrown, not swallowed")
    func aRefusedWriteIsThrown() async {
        let recorder = SourceSwitchRecorder()
        recorder.refusal = StubError.notImplemented

        await #expect(throws: StubError.self) {
            try await recorder.importer.setMetadataSourceEnabled(
                .discogs,
                false
            )
        }
    }

    /// Every string these switches introduced ships translated everywhere the
    /// rest of the catalog does.
    @Test("the source-switch strings ship in every locale")
    func theSourceSwitchStringsShipInEveryLocale() throws {
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

        let reference = try locales("Search")
        for key in [
            "Search %@",
            "No source to search",
            "Add %@ to find more pressings",
            "Many regional releases, reissues and vinyl pressings are only on %@. Tokens are free: create one in your %@ account settings.",
        ] {
            #expect(try locales(key) == reference, "\(key) is missing locales")
        }
    }
}

/// Collects what the source checkboxes write, and optionally refuses the way
/// core does when a write would leave nothing to ask.
@MainActor
private final class SourceSwitchRecorder {
    struct Write {
        let source: BridgeLookupCatalog
        let enabled: Bool
    }

    var writes: [Write] = []
    var refusal: (any Error)?

    var importer: Importer {
        Importer(
            setMetadataSourceEnabled: { [self] source, enabled in
                if let refusal { throw refusal }
                writes.append(Write(source: source, enabled: enabled))
            }
        )
    }
}
