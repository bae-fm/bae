import SwiftUI

/// Export preferences: the single-track "Save As…" suggested-filename template
/// and metadata selection, plus the release-export destination policy. Track
/// controls write through the `Exports` service and round-trip back via a
/// `configChanged` event into `ConfigStore` — no optimistic local mutation.
struct ExportSettingsTab: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Exports.self)
    private var exports
    @Environment(UiStore.self)
    private var uiStore

    /// Editable copy of the filename template, seeded from config and saved on
    /// submit. Kept as a draft so mid-edit keystrokes don't churn config.
    @State
    private var templateDraft = ""

    var body: some View {
        Form {
            Section("Release exports") {
                ExportLocationPicker(
                    configStore: configStore,
                    setLocation: exports.setExportLocation,
                    showError: { @MainActor error in uiStore.showError(error) }
                )
            }

            Section {
                LabeledContent("Filename format") {
                    HStack(spacing: 8) {
                        TextField("Filename format", text: $templateDraft)
                            .labelsHidden()
                            .textFieldStyle(.roundedBorder)
                            .onSubmit(saveTemplate)
                        Button("Save", action: saveTemplate)
                    }
                }
                Toggle("Title", isOn: metadataBinding(\.title))
                Toggle("Artist", isOn: metadataBinding(\.artist))
                Toggle("Album", isOn: metadataBinding(\.album))
                Toggle("Year", isOn: metadataBinding(\.year))
                Toggle("Track number", isOn: metadataBinding(\.trackNumber))
                Toggle("Disc number", isOn: metadataBinding(\.discNumber))
                Toggle("Cover art", isOn: metadataBinding(\.coverArt))
            } header: {
                Text("Track exports")
            } footer: {
                Text(
                    "Available tokens: {title} {artist} {album} {year} {track_number} {disc_number} {track_total}"
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .formStyle(.grouped)
        .task(id: configStore.config.exportFilenameTemplate) {
            templateDraft = configStore.config.exportFilenameTemplate
        }
    }

    private func saveTemplate() {
        set { try exports.setExportFilenameTemplate(templateDraft) }
    }

    /// A toggle binding that reads one metadata field and, on change, sends the
    /// whole updated selection to core (set-state, not toggle). The change
    /// round-trips through `configChanged`.
    private func metadataBinding(
        _ field: WritableKeyPath<BridgeExportMetadata, Bool>
    ) -> Binding<Bool> {
        Binding(
            get: { configStore.config.exportMetadata[keyPath: field] },
            set: { enabled in
                var updated = configStore.config.exportMetadata
                updated[keyPath: field] = enabled
                set { try exports.setExportMetadata(updated) }
            }
        )
    }

    private func set(_ apply: () throws -> Void) {
        do {
            try apply()
        }
        catch let error as BridgeError {
            uiStore.showError(DisplayError(error))
        }
        catch {
            uiStore.showError(DisplayError(line: error.localizedDescription))
        }
    }
}

#Preview("Export Settings") {
    ExportSettingsTab()
        .environment(PreviewData.configStore)
        .environment(Exports.stub)
        .environment(UiStore())
        .frame(width: 500, height: 500)
}
