import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing

@testable import bae

/// The per-source checkboxes on the Find online header, and what the pane says
/// when they leave nothing to ask.
@MainActor
@Suite("The Find online source switches")
struct FindOnlineSourceSwitchTests {
    private static let headerSize = NSSize(width: 660, height: 42)
    private static let paneSize = NSSize(width: 900, height: 620)

    /// One checkbox per source, named by the source. The names are brands, so
    /// they read the same in every language and the header can be checked for
    /// them literally — by containment, because SwiftUI's checkbox is an
    /// AppKit button with no title and no accessibility name, so the name is
    /// drawn beside the box and comes back glued to it ("v Discogs").
    @Test("the header names every source core reports")
    func theHeaderNamesEverySource() async throws {
        let lines = try await FindOnlineRendering.text(
            header(configStore: PreviewData.configStore()),
            size: Self.headerSize
        )

        for source in [BridgeMetadataSource.musicBrainz, .discogs] {
            let name = bridgeMetadataSourceName(source: source)
            #expect(
                lines.contains { $0.localizedCaseInsensitiveContains(name) },
                "the header reads: \(lines)"
            )
        }
    }

    /// A checked box and an unchecked one are different pixels — which is the
    /// only thing that tells a person which sources are being asked.
    @Test("a source switched off draws differently from one being asked")
    func aSwitchedOffSourceDrawsDifferently() async throws {
        let bothOn = try await pixels(of: PreviewData.configStore())
        let oneOff = try await pixels(
            of: PreviewData.makeConfigStore(
                libraryFullWidth: false,
                musicBrainz: .off
            )
        )

        #expect(bothOn != oneOff)
    }

    /// A source with no credential and a source the person switched off are
    /// both unchecked, and they are not the same thing: one cannot be moved.
    @Test("an unreachable source is not drawn as a switched-off one")
    func anUnreachableSourceIsNotASwitchedOffOne() async throws {
        // Discogs off but reachable, against Discogs with no token: both
        // unchecked, and only one of them can be checked again.
        let off = try await pixels(
            of: PreviewData.makeConfigStore(
                libraryFullWidth: false,
                discogs: .off
            )
        )
        let unreachable = try await pixels(
            of: PreviewData.makeConfigStore(
                libraryFullWidth: false,
                discogsUsable: false
            )
        )

        #expect(off != unreachable)
    }

    /// Both surfaces that carry these checkboxes — the header and the settings
    /// tab — write through the same Importer call, which passes the source and
    /// the value it is being set to straight through. Absolute, not a flip, so
    /// two windows disagreeing cannot leave the setting inverted.
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

        let reference = try locales("Find online")
        for key in [
            "Search %@",
            "No source to search",
            "Find online asks the sources that are checked here. The same checkboxes are on the Find online header.",
            "Add a %@ token to look up its releases too.",
        ] {
            #expect(try locales(key) == reference, "\(key) is missing locales")
        }
    }

    /// With every source switched off or unreachable there is nothing to look
    /// up, so the pane says so instead of offering an Identify button that
    /// would start a run core refuses to start.
    @Test("a pane with no source to ask says so instead of offering Identify")
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
        let identify = String(localized: "Identify")
        #expect(
            nothingToAsk.contains {
                $0.localizedCaseInsensitiveContains(noSource)
            },
            "a pane with nothing to ask reads: \(nothingToAsk)"
        )
        #expect(
            ordinary.contains { $0.localizedCaseInsensitiveContains(identify) },
            "an ordinary pane reads: \(ordinary)"
        )
    }

    /// A run that asked one source lays out one column. The ledger's rails and
    /// cells are sized off the provider list, so a list of one has to draw as
    /// a narrower table rather than an empty or a two-column one.
    @Test("a run on one source lays out one column")
    func aRunOnOneSourceLaysOutOneColumn() async throws {
        let size = NSSize(width: 660, height: 260)
        func ledger(_ run: BridgeIdentifyRun) async throws -> Data {
            try await FindOnlineRendering.pixels(
                IdentifyLedgerView(
                    run: run,
                    filePaths: [:],
                    onToggleCatalog: { _ in },
                    onRetryFailed: {}
                ),
                size: size
            )
        }

        let oneSource = try await ledger(PreviewData.identifyRunOneSource)
        let bothSources = try await ledger(PreviewData.identifyRunStarting)
        let blank = try await FindOnlineRendering.pixels(
            Color.clear,
            size: size
        )

        #expect(oneSource != blank, "a one-source run still draws its ledger")
        #expect(oneSource != bothSources)
    }

    /// The header as the app builds it: the switches come off the config the
    /// app observes, and the writes go through the Importer.
    private func header(configStore: ConfigStore) -> some View {
        FindOnlineHeader(onBack: {})
            .environment(configStore)
            .environment(Importer.stub())
            .environment(UiStore())
    }

    private func pixels(of configStore: ConfigStore) async throws -> Data {
        try await FindOnlineRendering.pixels(
            header(configStore: configStore),
            size: Self.headerSize
        )
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
        let source: BridgeMetadataSource
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
