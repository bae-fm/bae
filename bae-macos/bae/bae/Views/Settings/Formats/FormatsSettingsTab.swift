import BaeKit
import SwiftUI

/// The Save as pane: the formats and the names, each written through
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
        Form {
            formatsSection
            namesSection
        }
        .formStyle(.grouped)
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
            VStack(alignment: .leading, spacing: ThemeSpace.section) {
                SaveNameField(
                    title: "File name",
                    tokens: configStore.config.filenameTokens,
                    setTokens: { tokens in
                        set { try await outputs.setFilenameTokens(tokens) }
                    },
                    preview: BridgeSaveFilenameToken.previewName(
                        tokens: configStore.config.filenameTokens,
                        fallback: .title
                    ) + nextFileExtension
                )
                SaveNameField(
                    title: "Folder name",
                    tokens: configStore.config.folderTokens,
                    setTokens: { tokens in
                        set { try await outputs.setFolderTokens(tokens) }
                    },
                    preview: BridgeSaveFolderToken.previewName(
                        tokens: configStore.config.folderTokens,
                        fallback: .album
                    )
                )
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        } header: {
            Text("Names")
        }
    }

    private var formatsSection: some View {
        Section {
            ForEach(configStore.config.savePresets, id: \.id) { preset in
                PresetRow(
                    preset: preset,
                    edit: { editingPreset = EditingPreset(id: preset.id) },
                    delete: { deletePreset(id: preset.id) }
                )
            }
        } header: {
            Text("Formats")
        } footer: {
            Button("Add format…", action: addPreset)
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
        let preset = outputs.newSavePreset(configStore.config.savePresets)
        let presets = configStore.config.savePresets + [preset]
        set {
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
            .buttonStyle(.plain)
            MoreMenu {
                Button("Edit…", action: edit)
                Button("Delete…") {
                    confirmingDelete = true
                }
            }
            .confirmationDialog(
                "Delete the “\(preset.name)” format?",
                isPresented: $confirmingDelete
            ) {
                Button("Delete", role: .destructive, action: delete)
            }
        }
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
