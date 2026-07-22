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

    /// The preset whose edit sheet is open. A wrapper so `sheet(item:)` gets
    /// an `Identifiable` value; the sheet reads the live preset from config by
    /// this id, so edits round-tripping through `configChanged` re-render it.
    private struct EditingPreset: Identifiable {
        let id: String
    }

    @State
    private var editingPreset: EditingPreset?

    var body: some View {
        Form {
            releaseExportsSection
            trackExportsSection
            presetsSection
        }
        .formStyle(.grouped)
        .sheet(item: $editingPreset) { editing in
            // The preset can be gone by the time the sheet renders — deleted
            // from the sheet itself, or replaced by a config round-trip.
            if let preset = configStore.config.exportPresets.first(where: {
                $0.id == editing.id
            }) {
                ExportPresetEditor(preset: preset, update: replacePreset)
            }
        }
    }

    private var releaseExportsSection: some View {
        Section("Release exports") {
            presetPicker(
                presets: releasePresets,
                selection: defaultReleasePresetBinding()
            )
        }
    }

    private var trackExportsSection: some View {
        Section("Track exports") {
            presetPicker(
                presets: trackPresets,
                selection: defaultTrackPresetBinding()
            )
        }
    }

    private var presetsSection: some View {
        Section {
            ForEach(configStore.config.exportPresets, id: \.id) { preset in
                PresetRow(
                    preset: preset,
                    edit: { editingPreset = EditingPreset(id: preset.id) },
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

    private func presetPicker(
        presets: [BridgeExportPreset],
        selection: Binding<String>
    ) -> some View {
        Picker("Default format", selection: selection) {
            ForEach(presets, id: \.id) { preset in
                Text(preset.name).tag(preset.id)
            }
        }
    }

    private var trackPresets: [BridgeExportPreset] {
        configStore.config.exportPresets.filter(\.appliesToTrack)
    }

    private var releasePresets: [BridgeExportPreset] {
        configStore.config.exportPresets.filter(\.appliesToRelease)
    }

    private func replacePreset(_ preset: BridgeExportPreset) {
        let presets = configStore.config.exportPresets.map {
            $0.id == preset.id ? preset : $0
        }
        set { try exports.setExportPresets(presets) }
    }

    private func deletePreset(id: String) {
        let presets = configStore.config.exportPresets.filter { $0.id != id }
        set { try exports.setExportPresets(presets) }
    }

    /// The filename pattern a newly added preset starts with — the zero-padded
    /// track number then the title, matching core's default. The user edits it
    /// in the preset editor; there is no global pattern anymore.
    private static let defaultPresetFilenameTokens:
        [BridgeExportFilenameToken] =
            [.trackNumber, .title]

    private func addPreset(_ kind: ExportPresetKind) {
        let codec = kind.defaultCodec
        let preset = BridgeExportPreset(
            id: UUID().uuidString.replacingOccurrences(of: "-", with: ""),
            name: kind.label,
            codec: codec,
            extension: codec.fileExtension,
            filenameTokens: Self.defaultPresetFilenameTokens,
            pregapPlacement: .appendToPreviousExceptHtoa,
            appliesToTrack: true,
            appliesToRelease: true,
            embedCover: true
        )
        set {
            try exports.setExportPresets(
                configStore.config.exportPresets + [preset]
            )
        }
    }

    private func defaultTrackPresetBinding() -> Binding<String> {
        Binding(
            get: { configStore.config.defaultTrackSavePreset },
            set: { id in
                set { try exports.setDefaultTrackSavePreset(id) }
            }
        )
    }

    private func defaultReleasePresetBinding() -> Binding<String> {
        Binding(
            get: { configStore.config.defaultReleaseSavePreset },
            set: { id in
                set { try exports.setDefaultReleaseSavePreset(id) }
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

/// One preset in the settings list: the summary row opens the edit sheet; the
/// trailing minus removes the preset behind a confirmation.
private struct PresetRow: View {
    let preset: BridgeExportPreset
    let edit: () -> Void
    let delete: () -> Void

    @State
    private var confirmingDelete = false

    var body: some View {
        HStack(spacing: 12) {
            Button(action: edit) {
                ExportPresetSummaryRow(preset: preset)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            Button {
                confirmingDelete = true
            } label: {
                Image(systemName: "minus")
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .accessibilityLabel(Text("Delete preset"))
            .confirmationDialog(
                "Delete the “\(preset.name)” preset?",
                isPresented: $confirmingDelete
            ) {
                Button("Delete", role: .destructive, action: delete)
            }
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
