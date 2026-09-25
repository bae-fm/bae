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
    private static let paneSize = NSSize(width: 900, height: 620)

    /// The settings tab's checkboxes write through an Importer call that
    /// passes the source and the value it is being set to straight through.
    /// Absolute, not a flip, so two windows disagreeing cannot leave the
    /// setting inverted.
    @Test("the write carries the source and the value it is being set to")
    func theWriteCarriesTheValueItIsSetTo() throws {
        let recorder = SourceSwitchRecorder()
        let importer = recorder.importer

        try importer.setMetadataSourceEnabled(.discogs, false)
        try importer.setMetadataSourceEnabled(.musicBrainz, true)

        #expect(recorder.writes.map(\.source) == [.discogs, .musicBrainz])
        #expect(recorder.writes.map(\.enabled) == [false, true])
    }

    /// A refusal from core reaches the surface, which is what puts the checkbox
    /// back where it was and shows the reason.
    @Test("a refused write is thrown, not swallowed")
    func aRefusedWriteIsThrown() {
        let recorder = SourceSwitchRecorder()
        recorder.refusal = StubError.notImplemented

        #expect(throws: StubError.self) {
            try recorder.importer.setMetadataSourceEnabled(.discogs, false)
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
            "Add a %@ token to look up its releases too.",
        ] {
            #expect(try locales(key) == reference, "\(key) is missing locales")
        }
    }

    /// With every source switched off or unreachable there is nothing to look
    /// up, so the pane says so instead of offering the typed search, which
    /// would ask providers core will not ask.
    @Test("a pane with no source to ask says so instead of offering a search")
    func aPaneWithNoSourceSaysSo() async throws {
        let nothingToAsk = try await paneText(
            configStore: PreviewData.makeConfigStore(
                libraryFullWidth: false,
                discogsUsable: false,
                musicBrainz: .off
            )
        )
        let ordinary = try await paneText(
            configStore: PreviewData.makeConfigStore(libraryFullWidth: false)
        )

        let noSource = String(localized: "No source to search")
        let searchManually = String(localized: "Search manually")
        #expect(
            nothingToAsk.contains {
                $0.localizedCaseInsensitiveContains(noSource)
            },
            "a pane with nothing to ask reads: \(nothingToAsk)"
        )
        // With a source to ask, the section offers the other way to a release
        // rather than saying there is nothing to search.
        #expect(
            ordinary.contains {
                $0.localizedCaseInsensitiveContains(searchManually)
            },
            "an ordinary pane reads: \(ordinary)"
        )
    }

    /// A run that asked one source names that source on its chips. The
    /// capsules come off the provider list a cell at a time, so a run on one
    /// source has to draw its band rather than nothing, and differently from a
    /// run on both.
    @Test("a run on one source draws its band")
    func aRunOnOneSourceDrawsItsBand() async throws {
        let size = NSSize(width: 660, height: 260)
        func band(_ run: BridgeIdentifyRun) async throws -> Data {
            try await FindOnlineRendering.pixels(
                IdentifierBand(
                    run: run,
                    catalogAgreements: [],
                    onToggleLookup: { _ in },
                    onToggleCatalogAgreement: { _ in },
                    onRetryFailed: {},
                    onEditTitleSearch: { _, _ in }
                ),
                size: size
            )
        }

        let oneSource = try await band(PreviewData.identifyRunOneSource)
        let bothSources = try await band(PreviewData.identifyRunStarting)
        let blank = try await FindOnlineRendering.pixels(
            Color.clear,
            size: size
        )

        #expect(oneSource != blank, "a one-source run still draws its band")
        #expect(oneSource != bothSources)
    }

    /// Every line of text the pane draws for a library with these sources,
    /// with nothing looked up yet.
    private func paneText(configStore: ConfigStore) async throws -> [String] {
        try await FindOnlineRendering.text(
            ImportSearchPane.preview(state: PreviewData.searchStateIdle)
                .environment(configStore)
                .importPreviewEnvironment(),
            size: Self.paneSize
        )
    }

}

/// Collects what the source checkboxes write, and optionally refuses the way
/// core does when a write would leave nothing to ask.
@MainActor
private final class SourceSwitchRecorder {
    struct Write {
        let source: BridgeCatalog
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
