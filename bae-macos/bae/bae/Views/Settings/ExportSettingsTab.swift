import BaeKit
import SwiftUI

/// Export preferences: the release-export destination and default format, the
/// single-track suggested-filename pattern and default format, and the export
/// presets. Every control writes through the `Exports` service and round-trips
/// back via a `configChanged` event into `ConfigStore` — no optimistic local
/// mutation and no separate save step.
struct ExportSettingsTab: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Exports.self)
    private var exports
    @Environment(UiStore.self)
    private var uiStore

    @State
    private var expandedPresetIds: Set<String> = []

    var body: some View {
        Form {
            releaseExportsSection
            trackExportsSection
            presetsSection
        }
        .formStyle(.grouped)
    }

    private var releaseExportsSection: some View {
        Section("Release exports") {
            ExportLocationPicker(
                configStore: configStore,
                setLocation: exports.setExportLocation,
                showError: { @MainActor error in uiStore.showError(error) }
            )
            selectionPicker(
                presets: releasePresets,
                selection: defaultReleaseSelectionBinding()
            )
        }
    }

    private var trackExportsSection: some View {
        Section {
            selectionPicker(
                presets: trackPresets,
                selection: defaultTrackSelectionBinding()
            )
            VStack(alignment: .leading, spacing: 8) {
                Text("Filename format")
                FilenameTokenEditor(
                    tokens: configStore.config.exportFilenameTokens,
                    setTokens: { tokens in
                        set { try exports.setExportFilenameTokens(tokens) }
                    }
                )
            }
        } header: {
            Text("Track exports")
        } footer: {
            Text("Preview: \(Text(trackPreviewFilename).monospaced())")
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var presetsSection: some View {
        Section {
            ForEach(configStore.config.exportPresets, id: \.id) { preset in
                ExportPresetEditor(
                    preset: preset,
                    isExpanded: expandedBinding(preset.id),
                    update: replacePreset,
                    delete: { deletePreset(id: preset.id) }
                )
            }
            Menu("Add preset") {
                ForEach(ExportPresetKind.allCases, id: \.self) { kind in
                    Button(kind.label) { addPreset(kind) }
                }
            }
        } header: {
            Text("Presets")
        } footer: {
            Text("Presets appear in the export menu on tracks and releases.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func selectionPicker(
        presets: [BridgeExportPreset],
        selection: Binding<BridgeExportSelection>
    ) -> some View {
        Picker("Default format", selection: selection) {
            Text("Original").tag(BridgeExportSelection.original)
            ForEach(presets, id: \.id) { preset in
                Text(preset.name)
                    .tag(BridgeExportSelection.preset(presetId: preset.id))
            }
        }
    }

    private var trackPresets: [BridgeExportPreset] {
        configStore.config.exportPresets.filter(\.appliesToTrack)
    }

    private var releasePresets: [BridgeExportPreset] {
        configStore.config.exportPresets.filter(\.appliesToRelease)
    }

    /// The sample the pattern footer previews. "Original" keeps the source
    /// format, so the sample shows a representative extension; a preset
    /// default would still export whatever its own pattern renders, and the
    /// preset rows below preview that themselves.
    private var trackPreviewFilename: String {
        BridgeExportFilenameToken.previewFilename(
            tokens: configStore.config.exportFilenameTokens,
            fileExtension: "flac"
        )
    }

    private func expandedBinding(_ id: String) -> Binding<Bool> {
        Binding(
            get: { expandedPresetIds.contains(id) },
            set: { expanded in
                if expanded {
                    expandedPresetIds.insert(id)
                }
                else {
                    expandedPresetIds.remove(id)
                }
            }
        )
    }

    private func replacePreset(_ preset: BridgeExportPreset) {
        let presets = configStore.config.exportPresets.map {
            $0.id == preset.id ? preset : $0
        }
        set { try exports.setExportPresets(presets) }
    }

    private func deletePreset(id: String) {
        let presets = configStore.config.exportPresets.filter { $0.id != id }
        expandedPresetIds.remove(id)
        set { try exports.setExportPresets(presets) }
    }

    private func addPreset(_ kind: ExportPresetKind) {
        let codec = kind.defaultCodec
        let preset = BridgeExportPreset(
            id: UUID().uuidString.replacingOccurrences(of: "-", with: ""),
            name: kind.label,
            codec: codec,
            extension: codec.fileExtension,
            filenameTokens: configStore.config.exportFilenameTokens,
            pregapPlacement: .appendToPreviousExceptHtoa,
            appliesToTrack: true,
            appliesToRelease: true
        )
        expandedPresetIds.insert(preset.id)
        set {
            try exports.setExportPresets(
                configStore.config.exportPresets + [preset]
            )
        }
    }

    private func defaultTrackSelectionBinding() -> Binding<
        BridgeExportSelection
    > {
        Binding(
            get: { configStore.config.defaultTrackExportSelection },
            set: { selection in
                set { try exports.setDefaultTrackExportSelection(selection) }
            }
        )
    }

    private func defaultReleaseSelectionBinding()
        -> Binding<BridgeExportSelection>
    {
        Binding(
            get: { configStore.config.defaultReleaseExportSelection },
            set: { selection in
                set { try exports.setDefaultReleaseExportSelection(selection) }
            }
        )
    }

    private func set(_ apply: () throws -> Void) {
        do {
            try apply()
        }
        catch {
            uiStore.showError(error)
        }
    }
}

#if DEBUG
    #Preview("Export Settings") {
        ExportSettingsTab()
            .environment(PreviewData.configStore)
            .environment(Exports.stub)
            .environment(UiStore())
            .frame(width: 500, height: 600)
    }
#endif
