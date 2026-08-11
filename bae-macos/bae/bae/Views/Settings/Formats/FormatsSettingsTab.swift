import BaeKit
import SwiftUI

/// Export preferences: the export presets and the default preset for release
/// and track saves. Every control writes through the `Outputs` service and
/// round-trips back via a `configChanged` event into `ConfigStore` — no
/// optimistic local mutation and no separate save step.
struct FormatsSettingsTab: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Outputs.self)
    private var outputs
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
            formatsSection
            defaultsSection
        }
        .formStyle(.grouped)
        .sheet(item: $editingPreset) { editing in
            // The preset can be gone by the time the sheet renders — deleted
            // from the sheet itself, or replaced by a config round-trip.
            if let preset = configStore.config.savePresets.first(where: {
                $0.id == editing.id
            }) {
                SavePresetEditor(preset: preset, update: replacePreset)
            }
        }
    }

    private var defaultsSection: some View {
        Section {
            presetPicker(
                label: "Release",
                presets: releasePresets,
                selection: defaultReleasePresetBinding()
            )
            presetPicker(
                label: "Tracks",
                presets: trackPresets,
                selection: defaultTrackPresetBinding()
            )
        } header: {
            Text("Defaults")
        } footer: {
            Text("The format Save As… starts with.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var formatsSection: some View {
        Section {
            ForEach(configStore.config.savePresets, id: \.id) { preset in
                PresetRow(
                    preset: preset,
                    edit: { editingPreset = EditingPreset(id: preset.id) },
                    update: replacePreset,
                    delete: { deletePreset(id: preset.id) }
                )
            }
            Button(action: addPreset) {
                Label("Add format", systemImage: "plus")
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        } header: {
            Text("Formats")
        } footer: {
            Text("Formats appear in Save As… on tracks and releases.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func presetPicker(
        label: LocalizedStringKey,
        presets: [BridgeSavePreset],
        selection: Binding<String>
    ) -> some View {
        Picker(label, selection: selection) {
            ForEach(presets, id: \.id) { preset in
                Text(preset.name).tag(preset.id)
            }
        }
    }

    private var trackPresets: [BridgeSavePreset] {
        configStore.config.savePresets.filter(\.appliesToTrack)
    }

    private var releasePresets: [BridgeSavePreset] {
        configStore.config.savePresets.filter(\.appliesToRelease)
    }

    private func replacePreset(_ preset: BridgeSavePreset) {
        let presets = configStore.config.savePresets.map {
            $0.id == preset.id ? preset : $0
        }
        set { try outputs.setSavePresets(presets) }
    }

    private func deletePreset(id: String) {
        let presets = configStore.config.savePresets.filter { $0.id != id }
        set { try outputs.setSavePresets(presets) }
    }

    /// The filename pattern a newly added preset starts with — the zero-padded
    /// track number then the title, matching core's default. The user edits it
    /// in the preset editor; there is no global pattern anymore.
    private static let defaultPresetFilenameTokens: [BridgeSaveFilenameToken] =
        [.trackNumber, .title]

    /// Add a preset with the FLAC default and open its editor: the plus button
    /// creates the preset, then the user picks the format and settings in the
    /// sheet. The editor opens only after the save succeeds, so the sheet reads
    /// a preset that config already holds.
    private func addPreset() {
        let codec = SavePresetKind.flac.defaultCodec
        let preset = BridgeSavePreset(
            id: UUID().uuidString.replacingOccurrences(of: "-", with: ""),
            name: SavePresetKind.flac.label,
            codec: codec,
            extension: codec.fileExtension,
            filenameTokens: Self.defaultPresetFilenameTokens,
            pregapPlacement: .appendToPreviousExceptHtoa,
            appliesToTrack: true,
            appliesToRelease: true,
            embedCover: true
        )
        set {
            try outputs.setSavePresets(
                configStore.config.savePresets + [preset]
            )
            editingPreset = EditingPreset(id: preset.id)
        }
    }

    private func defaultTrackPresetBinding() -> Binding<String> {
        Binding(
            get: { configStore.config.defaultTrackSavePreset },
            set: { id in
                set { try outputs.setDefaultTrackSavePreset(id) }
            }
        )
    }

    private func defaultReleasePresetBinding() -> Binding<String> {
        Binding(
            get: { configStore.config.defaultReleaseSavePreset },
            set: { id in
                set { try outputs.setDefaultReleaseSavePreset(id) }
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

/// One format in the settings list: the summary opens the edit sheet, the
/// inline Track/Release toggles set which Save As… scopes it appears under, and
/// the trailing minus removes it behind a confirmation. A prop-driven leaf —
/// every scope change writes the whole updated preset back through `update`.
private struct PresetRow: View {
    let preset: BridgeSavePreset
    let edit: () -> Void
    let update: (BridgeSavePreset) -> Void
    let delete: () -> Void

    @State
    private var confirmingDelete = false

    var body: some View {
        HStack(spacing: 12) {
            Button(action: edit) {
                SavePresetSummaryRow(preset: preset)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            scopeToggles
            Button {
                confirmingDelete = true
            } label: {
                Image(systemName: "minus")
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .accessibilityLabel(Text("Delete format"))
            .confirmationDialog(
                "Delete the “\(preset.name)” format?",
                isPresented: $confirmingDelete
            ) {
                Button("Delete", role: .destructive, action: delete)
            }
        }
    }

    /// The Track and Release scope toggles. A single-file+CUE image is a
    /// whole-release export, so the pregap choice fixes its scope: Track reads
    /// off, Release reads on, and neither is editable here — the same gating
    /// the editor applies to its scope row.
    private var scopeToggles: some View {
        HStack(spacing: 8) {
            Toggle("Track", isOn: trackScopeBinding)
            Toggle("Release", isOn: releaseScopeBinding)
        }
        .toggleStyle(.button)
        .controlSize(.small)
        .disabled(preset.pregapPlacement == .singleFileWithCue)
    }

    private var trackScopeBinding: Binding<Bool> {
        Binding(
            get: { preset.trackScopeOn },
            set: { on in update(preset.withTrackScope(on)) }
        )
    }

    private var releaseScopeBinding: Binding<Bool> {
        Binding(
            get: { preset.releaseScopeOn },
            set: { on in update(preset.withReleaseScope(on)) }
        )
    }
}

#if DEBUG
    #Preview("Formats settings") {
        FormatsSettingsTab()
            .environment(PreviewData.configStore())
            .environment(Outputs.stub())
            .environment(UiStore())
            .frame(width: 500, height: 600)
    }
#endif
