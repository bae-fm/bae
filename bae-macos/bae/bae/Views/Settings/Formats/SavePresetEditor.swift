import BaeKit
import SwiftUI

/// The edit sheet for one export preset; every control writes the whole
/// preset through `update`, so Done only closes it.
struct SavePresetEditor: View {
    let preset: BridgeSavePreset
    /// Every format, this one included, for the names it must not repeat.
    let presets: [BridgeSavePreset]
    let update: (BridgeSavePreset) -> Void

    @Environment(\.dismiss)
    private var dismiss

    /// The name field, committed on submit or focus loss; an empty draft
    /// disables Done instead.
    @State
    private var nameDraft = ""
    @FocusState
    private var nameFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            Form {
                Section {
                    nameRow
                    formatRow
                    qualityRow
                }
            }
            .formStyle(.grouped)
            MacSheetFooter {
                Button("Done") { dismiss() }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(!nameValid)
            }
            .padding([.horizontal, .bottom], ThemeSpace.edge)
        }
        .frame(width: 480, height: 540)
        .task(id: preset.name) {
            nameDraft = preset.name
        }
    }

    private var nameValid: Bool {
        !nameDraft.trimmingCharacters(in: .whitespaces).isEmpty && !nameTaken
    }

    /// Another format already has the name in the field.
    private var nameTaken: Bool {
        bridgeSaveNameTaken(
            name: nameDraft,
            ownId: preset.id,
            presets: presets
        )
    }

    /// The names of the other formats.
    private var otherNames: [String] {
        presets.filter { $0.id != preset.id }.map(\.name)
    }

    /// The format's one quality setting: a bit depth for a lossless codec, a
    /// bitrate from the format's list for a lossy one.
    @ViewBuilder
    private var qualityRow: some View {
        if let bitDepth = preset.codec.bitDepth {
            PresetBitDepthRow(
                preset: preset,
                bitDepth: bitDepth,
                update: update
            )
        }
        else if let bitrate = preset.codec.bitrateKbps,
            let accepted = bridgeSaveFormatBitratesKbps(
                format: preset.codec.format
            )
        {
            PresetBitrateRow(
                preset: preset,
                bitrate: bitrate,
                accepted: accepted,
                update: update
            )
        }
    }

    private var nameRow: some View {
        LabeledContent("Name") {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                TextField("Name", text: $nameDraft)
                    .labelsHidden()
                    .textFieldStyle(.plain)
                    .multilineTextAlignment(.leading)
                    .focused($nameFocused)
                    .modifier(
                        FieldChrome(
                            focused: nameFocused,
                            style: .boxed,
                            invalid: nameTaken
                        )
                    )
                    .frame(width: 200)
                    .onSubmit(commitName)
                    .onChange(of: nameFocused) { _, focused in
                        if !focused {
                            commitName()
                        }
                    }
                if nameTaken {
                    Text("Another format is named “\(nameDraft)”.")
                        .themeText(.detail)
                        .foregroundStyle(Theme.danger)
                        .padding(.leading, ThemeSpace.fieldInset)
                }
            }
        }
        .presetEditorRowInsets()
    }

    /// The formats the picker offers, in core's order and groups.
    private static let formatGroups = bridgeSaveFormatGroups()

    private var formatRow: some View {
        LabeledContent("Format") {
            formatPicker
        }
        .presetEditorRowInsets()
    }

    private var formatPicker: some View {
        Picker(
            "Format",
            selection: Binding(
                get: { preset.codec.format },
                set: { format in
                    var changed = preset
                    changed.codec = preset.codec.switched(to: format)
                    // A name that is still a format's prefill follows the
                    // format; a name the user typed stays put.
                    changed.name = bridgeSaveNameAfterFormatChange(
                        name: preset.name,
                        from: preset.codec.format,
                        to: format,
                        taken: otherNames
                    )
                    update(changed)
                }
            )
        ) {
            ForEach(Self.formatGroups.indices, id: \.self) { group in
                if group > 0 {
                    Divider()
                }
                ForEach(Self.formatGroups[group], id: \.self) { format in
                    Text(format.label).tag(format)
                }
            }
        }
        .labelsHidden()
    }

    private func commitName() {
        guard nameValid, nameDraft != preset.name else { return }
        var changed = preset
        changed.name = nameDraft
        update(changed)
    }
}

#if DEBUG
    #Preview("Export preset editor") {
        @Previewable
        @State
        var preset = PreviewData.savePresets[0]

        SavePresetEditor(
            preset: preset,
            presets: PreviewData.savePresets,
            update: { preset = $0 }
        )
    }
#endif
