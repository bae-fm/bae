import BaeKit
import SwiftUI

/// The Save as pane: the formats, the names and the extras, each written through
/// `Outputs` and read back from config.
struct FormatsSettingsTab: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Outputs.self)
    private var outputs
    @Environment(UiStore.self)
    private var uiStore

    /// The id of the preset whose edit sheet is open; the sheet reads the live
    /// preset from config by it.
    private struct EditingPreset: Identifiable {
        let id: String
    }

    @State
    private var editingPreset: EditingPreset?

    var body: some View {
        SettingsPane(.formats) {
            Form {
                formatsSection
                namesSection
                extrasSection
            }
            .formStyle(.settings)
        }
        .sheet(item: $editingPreset) { editing in
            // The preset can be gone by the time the sheet renders — deleted
            // from the sheet itself, or replaced by a config round-trip.
            if let preset = configStore.config.savePresets.first(where: {
                $0.id == editing.id
            }) {
                SavePresetEditor(
                    preset: preset,
                    presets: configStore.config.savePresets,
                    update: replacePreset
                )
            }
        }
    }

    /// The extension, dot included, of the files the next save writes: those
    /// of the format last used, as the save panel starts with it.
    private var nextFileExtension: String {
        let config = configStore.config
        guard
            let preset = config.savePresets.first(where: {
                $0.id == config.lastSavePreset
            })
        else { return "" }
        return "." + preset.codec.fileExtension
    }

    private var namesSection: some View {
        Section {
            SaveNameField(
                title: "File name",
                pieces: configStore.config.filenamePieces,
                error: configStore.config.filenamePatternError,
                previewSuffix: nextFileExtension,
                setPieces: outputs.setFilenamePieces,
                fail: { uiStore.showError($0) }
            )
            SaveNameField(
                title: "Folder name",
                pieces: configStore.config.folderPieces,
                error: configStore.config.folderPatternError,
                setPieces: outputs.setFolderPieces,
                fail: { uiStore.showError($0) }
            )
        } header: {
            Text("Names")
        }
    }

    private var extrasSection: some View {
        Section {
            extraRow(
                "Embed cover art",
                isOn: configStore.config.saveExtras.embedCover,
                set: outputs.setSaveEmbedCover
            )
            extraRow(
                "Cover image in the folder",
                note:
                    "Adds cover.jpg, which many car stereos and players show.",
                isOn: configStore.config.saveExtras.coverFile,
                set: outputs.setSaveCoverFile
            )
            extraRow(
                "Loudness tags",
                note:
                    "Writes bae’s loudness measurements as ReplayGain, so other players even out the volume.",
                isOn: configStore.config.saveExtras.loudnessTags,
                set: outputs.setSaveLoudnessTags
            )
        } header: {
            Text("Extras")
        }
    }

    /// One switch of the Extras section, with the note that says what it does.
    private func extraRow(
        _ title: LocalizedStringKey,
        note: LocalizedStringKey? = nil,
        isOn: Bool,
        set write: @escaping @Sendable (Bool) async throws -> Void
    ) -> some View {
        SettingToggleRow(
            title: title,
            note: note,
            isOn: Binding(
                get: { isOn },
                set: { on in set { try await write(on) } }
            )
        )
    }

    private var formatsSection: some View {
        Section {
            ForEach(configStore.config.savePresets, id: \.id) { preset in
                PresetRow(
                    preset: preset,
                    canDelete: bridgeSavePresetCanDelete(
                        presets: configStore.config.savePresets
                    ),
                    edit: { editingPreset = EditingPreset(id: preset.id) },
                    delete: { deletePreset(id: preset.id) }
                )
            }
        } header: {
            Text("Formats")
        } footer: {
            Button("Add format…", action: addPreset)
                .themeText(.body)
                .frame(maxWidth: .infinity, alignment: .trailing)
        }
    }

    private func replacePreset(_ preset: BridgeSavePreset) {
        let presets = configStore.config.savePresets.map {
            $0.id == preset.id ? preset : $0
        }
        set { try await outputs.setSavePresets(presets) }
    }

    private func deletePreset(id: String) {
        let presets = configStore.config.savePresets.filter { $0.id != id }
        set { try await outputs.setSavePresets(presets) }
    }

    /// Adds the preset core makes for a new format and opens its editor once
    /// config holds it.
    private func addPreset() {
        set {
            let preset = try outputs.newSavePreset(
                configStore.config.savePresets
            )
            let presets = configStore.config.savePresets + [preset]
            try await outputs.setSavePresets(presets)
            editingPreset = EditingPreset(id: preset.id)
        }
    }

    /// Run a config write off the main thread — each is a durable file
    /// replace — and show a failure.
    private func set(_ apply: @escaping @MainActor () async throws -> Void) {
        Task {
            do {
                try await apply()
            }
            catch {
                uiStore.showError(error)
            }
        }
    }
}

/// One format in the settings list: the summary opens the edit sheet, and the
/// ⋯ menu offers Edit… and Delete….
private struct PresetRow: View {
    let preset: BridgeSavePreset
    /// Core's answer for the list: the only format left can't be deleted.
    let canDelete: Bool
    let edit: () -> Void
    let delete: () -> Void

    @State
    private var confirmingDelete = false

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            Button(action: edit) {
                SavePresetSummaryRow(preset: preset)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.accentPlain)
            MacMoreMenu {
                MacMenuItem("Edit…", action: edit)
                MacMenuItem("Delete…", disabled: !canDelete) {
                    confirmingDelete = true
                }
            }
            .macAlert(isPresented: $confirmingDelete) {
                DeleteFormatConfirmation.alert(named: preset.name) { delete() }
            }
        }
    }
}

/// The alert that asks before a format is deleted.
enum DeleteFormatConfirmation {
    /// The alert for the format called `name`; Delete runs `onDelete`.
    static func alert(
        named name: String,
        onDelete: @escaping @MainActor () -> Void
    ) -> (MacAlertWords, [MacAlertButton]) {
        (
            MacAlertWords(Text("Delete the \(name) format?")),
            [
                MacAlertButton(
                    Text("Delete"),
                    role: .destructive,
                    action: onDelete
                ),
                MacAlertButton(Text("Cancel"), role: .cancel),
            ]
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
            .windowBackground()
    }
#endif
