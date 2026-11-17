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

    /// The id of the preset whose edit sheet is open; the sheet reads the
    /// preset by it.
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
            SavePresetSheet(
                presetId: editing.id,
                presets: configStore.config.savePresets,
                update: replacePreset,
                onEnded: { editingPreset = nil }
            )
        }
    }

    /// The extension, dot included, of the files the next save writes: those
    /// of the format last used, as the save panel starts with it.
    private var nextFileExtension: String {
        "." + configStore.config.lastSavePreset.codec.fileExtension
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

    /// Write a change of a preset's codec or quality, which keeps its name.
    private func replacePreset(_ preset: BridgeSavePreset) {
        set {
            if try await outputs.replaceSavePreset(preset) == .nameTaken {
                throw SavePresetNameTaken(name: preset.name)
            }
        }
    }

    private func deletePreset(id: String) {
        set { try await outputs.deleteSavePreset(id) }
    }

    /// Adds the preset core makes for a new format and opens its editor once
    /// config holds it.
    private func addPreset() {
        set {
            let preset = try outputs.newSavePreset(
                configStore.config.savePresets
            )
            configStore.addedSavePresetId = preset.id
            do {
                if try await outputs.addSavePreset(preset) == .nameTaken {
                    throw SavePresetNameTaken(name: preset.name)
                }
            }
            catch {
                configStore.addedSavePresetId = nil
                throw error
            }
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

/// Core refused a preset's write because another preset has its name, where
/// the screen had no field to say so in.
private struct SavePresetNameTaken: LocalizedError {
    let name: String

    var errorDescription: String? {
        String(localized: "Another format is named \(name).")
    }
}

/// The edit sheet of the preset `presetId`, drawn from the preset's own live
/// read. The sheet closes when the read returns no preset for the id.
private struct SavePresetSheet: View {
    let presetId: String
    /// Every format, this one included, which the editor uses to pick the
    /// name a codec change gives this format.
    let presets: [BridgeSavePreset]
    let update: (BridgeSavePreset) -> Void
    let onEnded: () -> Void

    @Environment(Outputs.self)
    private var outputs
    @Environment(UiStore.self)
    private var uiStore
    @State
    private var read = SavePresetRead()

    var body: some View {
        // A stack, not a group: it is on screen before the read answers, so
        // its appearance starts the read.
        ZStack {
            if case .showing(let preset) = read.state {
                SavePresetEditor(
                    preset: preset,
                    presets: presets,
                    update: update,
                    rename: { try await outputs.replaceSavePreset($0) }
                )
            }
        }
        .onAppear {
            do {
                try read.open(
                    outputs.subscribeSavePreset(presetId),
                    fail: { uiStore.showError($0) }
                )
            }
            catch {
                uiStore.showError(error)
            }
        }
        .onDisappear { read.close() }
        .onChange(of: read.state) { _, state in
            if state == .ended { onEnded() }
        }
    }
}

/// What one save preset's live read has returned so far.
private enum SavePresetReadState: Equatable {
    /// The read has returned nothing yet.
    case waiting
    /// The read last returned this preset.
    case showing(BridgeSavePreset)
    /// The read returned that no preset has the id.
    case ended
}

/// One save preset's live read, while its edit sheet shows.
@MainActor
@Observable
private final class SavePresetRead {
    private(set) var state = SavePresetReadState.waiting
    @ObservationIgnored
    private var subscription: AnyObject?

    /// Follow `source`, handing a failed read to `fail`.
    func open(
        _ source: SavePresetSubscription,
        fail: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        subscription = ScreenSubscription(
            source,
            onValue: { [weak self] preset in
                self?.state = preset.map { .showing($0) } ?? .ended
            },
            onError: fail
        )
    }

    func close() {
        subscription = nil
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
