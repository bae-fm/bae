import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing

@testable import bae

/// The two import settings, and that each one writes the setting it names.
///
/// They are independent: the draft a candidate starts from and whether
/// identification runs on its own are separate answers, so the tab draws one
/// switch each and neither write carries the other's value.
@MainActor
@Suite("The import settings")
struct ImportSettingsTabTests {
    private static let size = NSSize(width: 520, height: 420)

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

    private func tab(recorder: ImportSettingRecorder) -> some View {
        ImportSettingsTab()
            .environment(PreviewData.configStore())
            .environment(Downloads.stub())
            .environment(Sync.stub())
            .environment(recorder.importer)
            .environment(UiStore())
    }

    private func switches(in host: NSView) -> [NSSwitch] {
        SnapshotTestSupport.descendants(of: host).compactMap { $0 as? NSSwitch }
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
