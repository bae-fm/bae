import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing

@testable import bae

/// The import settings, the sources, and the Discogs key that one of those
/// sources needs.
///
/// Each switch writes only its own setting and carries no other's value.
@MainActor
@Suite("The import settings")
struct ImportSettingsTabTests {
    private static let size = NSSize(width: 520, height: 900)

    @Test("the tab draws one switch per setting and one per source")
    func theTabDrawsOneSwitchPerSetting() async throws {
        let recorder = ImportSettingRecorder()
        try await SnapshotTestSupport.withHostedWindow(
            tab(recorder: recorder),
            size: Self.size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            // The two settings, plus the sources core reports — which are
            // core's list, not constants this tab repeats.
            let config = PreviewData.configStore().config
            #expect(config.lookupCatalogs.count == 2)
            #expect(
                switches(in: host).count == 2 + config.lookupCatalogs.count
            )
        }
    }

    @Test("each switch writes its own setting")
    func eachSwitchWritesItsOwnSetting() async throws {
        let recorder = ImportSettingRecorder()
        try await SnapshotTestSupport.withHostedWindow(
            tab(recorder: recorder),
            size: Self.size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            // Every switch starts on but importing when identified, so clicking
            // each one writes the other value for the setting it names and
            // says nothing about any other.
            for control in switches(in: host) {
                HostedInput.press(control)
                try await SnapshotTestSupport.settle(host)
            }

            #expect(recorder.identifyWrites == [false])
            #expect(recorder.importWhenIdentifiedWrites == [true])
            #expect(
                recorder.sourceWrites.map(\.enabled) == [false, false],
                "each source switch wrote the value it was set to"
            )
            #expect(
                Set(recorder.sourceWrites.map(\.source))
                    == Set(
                        PreviewData.configStore().config.lookupCatalogs
                            .map(\.catalog)
                    )
            )
        }
    }

    /// The two settings are independent: importing when identified covers
    /// identifications the person starts, so it can be turned on while
    /// identification does not run on its own.
    @Test("with identifying automatically off the import switch still writes")
    func theImportSwitchWritesWithIdentifyingAutomaticallyOff() async throws {
        let recorder = ImportSettingRecorder()
        try await SnapshotTestSupport.withHostedWindow(
            tab(
                recorder: recorder,
                configStore: PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    identifyAutomatically: false
                )
            ),
            size: Self.size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            // Importing when identified is the pane's last switch, in the box
            // under the one holding identification and the sources.
            let importSwitch = try #require(switches(in: host).last)
            #expect(importSwitch.isEnabled)
            HostedInput.press(importSwitch)
            try await SnapshotTestSupport.settle(host)

            #expect(recorder.importWhenIdentifiedWrites == [true])
            #expect(recorder.identifyWrites.isEmpty)
        }
    }

    /// Discogs cannot be asked without a key, so the key is offered where the
    /// switch it unlocks is, and core reports the switch as immovable until one
    /// is stored.
    @Test("with no key stored the Sources section offers the key input")
    func theSourcesSectionOffersTheKeyInput() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            tab(
                recorder: ImportSettingRecorder(),
                configStore: PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    discogsUsable: false
                )
            ),
            size: Self.size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            #expect(keyField(in: host) != nil)
            #expect(discogsSwitch(in: host)?.isEnabled == false)
        }
    }

    @Test("a stored key replaces the input with what the key is doing")
    func aStoredKeyReplacesTheInput() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            tab(recorder: ImportSettingRecorder()),
            size: Self.size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            #expect(keyField(in: host) == nil)
            #expect(discogsSwitch(in: host)?.isEnabled == true)
        }
    }

    /// How many files move at once is its own pane: it is not a metadata
    /// answer, and unlike everything here it stays on this device.
    @Test("the tab carries no transfer controls")
    func theTabCarriesNoTransferControls() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            tab(recorder: ImportSettingRecorder()),
            size: Self.size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            #expect(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSSegmentedControl }
                    .isEmpty
            )
        }
    }

    @MainActor
    private func tab(
        recorder: ImportSettingRecorder,
        configStore: ConfigStore? = nil
    ) -> some View {
        ImportSettingsTab()
            .environment(configStore ?? PreviewData.configStore())
            .environment(Discogs(getDiscogsToken: { "stored-key" }))
            .environment(recorder.importer)
            .environment(UiStore())
    }

    private func switches(in host: NSView) -> [NSSwitch] {
        SnapshotTestSupport.descendants(of: host).compactMap { $0 as? NSSwitch }
    }

    /// Core lists the sources in its own order and Discogs is the last of
    /// them, so the switch just above the one importing when identified is the
    /// one the key unlocks.
    private func discogsSwitch(in host: NSView) -> NSSwitch? {
        switches(in: host).dropLast().last
    }

    private func keyField(in host: NSView) -> NSTextField? {
        SnapshotTestSupport.descendants(of: host)
            .compactMap { $0 as? NSTextField }
            .first {
                $0.placeholderString
                    == String(localized: "Paste your key here")
            }
    }
}

@MainActor
private final class ImportSettingRecorder {
    var identifyWrites: [Bool] = []
    var sourceWrites: [(source: BridgeLookupCatalog, enabled: Bool)] = []
    var importWhenIdentifiedWrites: [Bool] = []

    var importer: Importer {
        Importer(
            setIdentifyAutomatically: { [self] in identifyWrites.append($0) },
            setMetadataSourceEnabled: { [self] source, enabled in
                sourceWrites.append((source: source, enabled: enabled))
            },
            setImportWhenIdentified: { [self] in
                importWhenIdentifiedWrites.append($0)
            }
        )
    }
}
