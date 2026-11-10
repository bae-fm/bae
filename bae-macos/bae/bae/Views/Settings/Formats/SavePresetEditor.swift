import BaeKit
import SwiftUI

/// The edit sheet for one export preset; every control writes the whole
/// preset through `update`, so Done only closes it.
struct SavePresetEditor: View {
    let preset: BridgeSavePreset
    let update: (BridgeSavePreset) -> Void

    @Environment(\.dismiss)
    private var dismiss

    /// The free-form fields, committed on submit or focus loss; an invalid
    /// draft disables Done instead.
    @State
    private var nameDraft = ""
    @State
    private var bitrateDraft = ""
    @State
    private var advancedExpanded = false
    @FocusState
    private var nameFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            Form {
                Section {
                    nameRow
                    formatRow
                    if preset.codec.bitrateKbps != nil {
                        PresetBitrateRow(
                            preset: preset,
                            bitrateDraft: $bitrateDraft,
                            update: update
                        )
                    }
                }
                Section {
                    coverRow
                }
                FormDisclosure(isExpanded: $advancedExpanded) {
                    if let bitDepth = preset.codec.bitDepth {
                        PresetBitDepthRow(
                            preset: preset,
                            bitDepth: bitDepth,
                            update: update
                        )
                    }
                    pregapRow
                } label: {
                    Text("Advanced")
                }
            }
            .formStyle(.grouped)
            MacSheetFooter {
                Button("Done") { dismiss() }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(!nameValid || !bitrateValid)
            }
            .padding([.horizontal, .bottom], ThemeSpace.edge)
        }
        .frame(width: 480, height: 540)
        .task(id: preset.name) {
            nameDraft = preset.name
        }
        .task(id: preset.codec.bitrateKbps) {
            if let bitrateKbps = preset.codec.bitrateKbps {
                bitrateDraft = String(bitrateKbps)
            }
        }
    }

    private var nameValid: Bool {
        !nameDraft.trimmingCharacters(in: .whitespaces).isEmpty
    }

    /// The bitrate draft parses and lands in the codec's supported range;
    /// lossless codecs carry no bitrate to validate.
    private var bitrateValid: Bool {
        guard
            let accepted = bridgeSaveFormatBitratesKbps(
                format: preset.codec.format
            )
        else { return true }
        guard let value = UInt32(bitrateDraft) else { return false }
        return (accepted.min...accepted.max).contains(value)
    }

    private var nameRow: some View {
        LabeledContent("Name") {
            TextField("Name", text: $nameDraft)
                .labelsHidden()
                .textFieldStyle(.plain)
                .multilineTextAlignment(.leading)
                .focused($nameFocused)
                .modifier(FieldChrome(focused: nameFocused, style: .boxed))
                .frame(width: 200)
                .onSubmit(commitName)
                .onChange(of: nameFocused) { _, focused in
                    if !focused {
                        commitName()
                    }
                }
        }
        .presetEditorRowInsets()
    }

    /// The formats the picker offers, in core's order.
    private static let formats = bridgeSaveFormats()

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
                    // A default name follows the format; a name the user typed
                    // stays put.
                    if preset.name == preset.codec.format.label {
                        changed.name = format.label
                    }
                    if changed.pregapPlacement == .singleFileWithCue,
                        !changed.codec.supportsSingleFileCue
                    {
                        changed.pregapPlacement = .appendToPreviousExceptHtoa
                    }
                    update(changed)
                }
            )
        ) {
            ForEach(Self.formats, id: \.self) { format in
                Text(format.label).tag(format)
            }
        }
        .labelsHidden()
    }

    private var pregapRow: some View {
        LabeledContent("Pregap") {
            pregapPicker
        }
        .presetEditorRowInsets()
    }

    private var pregapPicker: some View {
        Picker(
            "Pregap",
            selection: Binding(
                get: { preset.pregapPlacement },
                set: { placement in
                    var changed = preset
                    changed.pregapPlacement = placement
                    update(changed)
                }
            )
        ) {
            Text("Append except HTOA")
                .tag(BridgeSavePregapPlacement.appendToPreviousExceptHtoa)
            Text("Append including HTOA")
                .tag(BridgeSavePregapPlacement.appendToPreviousIncludingHtoa)
            Text("Exclude")
                .tag(BridgeSavePregapPlacement.exclude)
            if preset.codec.supportsSingleFileCue {
                Text("Single file + CUE")
                    .tag(BridgeSavePregapPlacement.singleFileWithCue)
            }
        }
        .labelsHidden()
    }

    private var coverRow: some View {
        Toggle("Embed cover art", isOn: embedCoverBinding)
            .presetEditorRowInsets()
    }

    private var embedCoverBinding: Binding<Bool> {
        Binding(
            get: { preset.embedCover },
            set: { enabled in
                var changed = preset
                changed.embedCover = enabled
                update(changed)
            }
        )
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
            update: { preset = $0 }
        )
    }
#endif
