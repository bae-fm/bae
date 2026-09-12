import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing

@testable import bae

/// The two import settings, the sources, and the Discogs key that one of those
/// sources needs.
///
/// The settings are independent: the draft a candidate starts from and whether
/// identification runs on its own are separate answers, so the tab draws one
/// switch each and neither write carries the other's value.
@MainActor
@Suite("The import settings")
struct ImportSettingsTabTests {
    private static let size = NSSize(width: 520, height: 560)

    @Test("the tab draws one switch per setting and one per source")
    func theTabDrawsOneSwitchPerSetting() async throws {
        let recorder = ImportSettingRecorder()
        let (window, host) = SnapshotTestSupport.hostInWindow(
            tab(recorder: recorder),
            size: Self.size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)

        // The two settings, plus the sources core reports — which are core's
        // list, not a constant this tab repeats.
        let sources = PreviewData.configStore().config.metadataSources
        #expect(sources.count == 2)
        #expect(switches(in: host).count == 2 + sources.count)
    }

    @Test("each switch writes its own setting")
    func eachSwitchWritesItsOwnSetting() async throws {
        let recorder = ImportSettingRecorder()
        let (window, host) = SnapshotTestSupport.hostInWindow(
            tab(recorder: recorder),
            size: Self.size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)

        // Every switch starts on, so clicking each one writes `false` for the
        // setting it names and says nothing about any other.
        for control in switches(in: host) {
            control.performClick(nil)
            await SnapshotTestSupport.settle(host)
        }

        #expect(recorder.prefillWrites == [false])
        #expect(recorder.identifyWrites == [false])
        #expect(
            recorder.sourceWrites.map(\.enabled) == [false, false],
            "each source switch wrote the value it was set to"
        )
        #expect(
            Set(recorder.sourceWrites.map(\.source))
                == Set(
                    PreviewData.configStore().config.metadataSources
                        .map(\.source)
                )
        )
    }

    /// Both sentences under the switches say what happens when a candidate is
    /// added — the moment either setting acts.
    @Test("the tab names both settings")
    func theTabNamesBothSettings() async throws {
        let lines = try await FindOnlineRendering.text(
            tab(recorder: ImportSettingRecorder()),
            size: Self.size
        )

        for label in [
            String(localized: "Pre-fill with tags"),
            String(localized: "Identify automatically"),
        ] {
            #expect(
                lines.contains { $0.localizedCaseInsensitiveContains(label) },
                "the tab reads: \(lines)"
            )
        }
    }

    /// Discogs cannot be asked without a key, so the key is offered where the
    /// switch it unlocks is, and core reports the switch as immovable until one
    /// is stored.
    @Test("with no key stored the Sources section offers the key input")
    func theSourcesSectionOffersTheKeyInput() async throws {
        let (window, host) = SnapshotTestSupport.hostInWindow(
            tab(
                recorder: ImportSettingRecorder(),
                configStore: PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    discogsUsable: false
                )
            ),
            size: Self.size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)

        #expect(keyField(in: host) != nil)
        // Core lists the sources in its own order and Discogs is the last of
        // them, so the last switch on the pane is the one the key unlocks.
        #expect(switches(in: host).last?.isEnabled == false)

        let lines = try await FindOnlineRendering.text(
            tab(
                recorder: ImportSettingRecorder(),
                configStore: PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    discogsUsable: false
                )
            ),
            size: Self.size
        )
        #expect(reads(lines, String(localized: "Save")), "\(lines)")
    }

    @Test("a stored key replaces the input with what the key is doing")
    func aStoredKeyReplacesTheInput() async throws {
        let (window, host) = SnapshotTestSupport.hostInWindow(
            tab(recorder: ImportSettingRecorder()),
            size: Self.size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)

        #expect(keyField(in: host) == nil)
        #expect(switches(in: host).last?.isEnabled == true)

        let lines = try await FindOnlineRendering.text(
            tab(recorder: ImportSettingRecorder()),
            size: Self.size
        )
        for label in [
            String(localized: "Connected"),
            String(localized: "Remove"),
        ] {
            #expect(reads(lines, label), "\(lines)")
        }
    }

    /// How many files move at once is its own pane: it is not a metadata
    /// answer, and unlike everything here it stays on this device.
    @Test("the tab carries no transfer controls")
    func theTabCarriesNoTransferControls() async throws {
        let (window, host) = SnapshotTestSupport.hostInWindow(
            tab(recorder: ImportSettingRecorder()),
            size: Self.size
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        await SnapshotTestSupport.settle(host)

        #expect(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSSegmentedControl }
                .isEmpty
        )
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

    private func reads(_ lines: [String], _ label: String) -> Bool {
        lines.contains { $0.localizedCaseInsensitiveContains(label) }
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
    var prefillWrites: [Bool] = []
    var identifyWrites: [Bool] = []
    var sourceWrites: [(source: BridgeMetadataSource, enabled: Bool)] = []

    var importer: Importer {
        Importer(
            setIdentifyAutomatically: { [self] in identifyWrites.append($0) },
            setPrefillWithTags: { [self] in prefillWrites.append($0) },
            setMetadataSourceEnabled: { [self] source, enabled in
                sourceWrites.append((source: source, enabled: enabled))
            }
        )
    }
}
