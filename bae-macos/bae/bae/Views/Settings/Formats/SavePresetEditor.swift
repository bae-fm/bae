import BaeKit
import SwiftUI

/// The edit sheet for one export preset. Every control writes a whole updated
/// preset up through `update` — the caller saves the preset set and the change
/// round-trips via `configChanged`, so the sheet always renders the stored
/// state and Done only closes it. Deleting lives on the preset's list row,
/// not here.
struct SavePresetEditor: View {
    let preset: BridgeSavePreset
    let update: (BridgeSavePreset) -> Void

    @Environment(\.dismiss)
    private var dismiss

    /// Editable copies of the free-form fields, committed on submit or focus
    /// loss so mid-edit keystrokes don't churn config writes. Invalid drafts
    /// never commit; they disable Done instead.
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
                Section("File naming") {
                    filenameGroup
                }
                Section {
                    coverRow
                    scopeRow
                }
                DisclosureGroup(isExpanded: $advancedExpanded) {
                    if preset.codec.bitDepth != nil {
                        PresetBitDepthRow(preset: preset, update: update)
                    }
                    pregapRow
                } label: {
                    Text("Advanced")
                }
                .presetEditorRowInsets()
            }
            .formStyle(.grouped)
            HStack {
                Spacer()
                Button("Done") { dismiss() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!nameValid || !bitrateValid)
            }
            .padding([.horizontal, .bottom], 16)
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
        guard let range = preset.codec.kind.bitrateRange else { return true }
        guard let value = UInt32(bitrateDraft) else { return false }
        return range.contains(value)
    }

    private var nameRow: some View {
        LabeledContent("Name") {
            TextField("Name", text: $nameDraft)
                .labelsHidden()
                .textFieldStyle(.roundedBorder)
                .multilineTextAlignment(.leading)
                .frame(width: 200)
                .focused($nameFocused)
                .onSubmit(commitName)
                .onChange(of: nameFocused) { _, focused in
                    if !focused {
                        commitName()
                    }
                }
        }
        .presetEditorRowInsets()
    }

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
                get: { preset.codec.kind },
                set: { kind in
                    var changed = preset
                    changed.codec = preset.codec.switched(to: kind)
                    changed.extension = changed.codec.fileExtension
                    // A default name follows the format; a name the user typed
                    // stays put.
                    if preset.name == preset.codec.kind.label {
                        changed.name = kind.label
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
            ForEach(SavePresetKind.allCases, id: \.self) { kind in
                Text(kind.label).tag(kind)
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
                    if placement == .singleFileWithCue {
                        changed.appliesToTrack = false
                        changed.appliesToRelease = true
                    }
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

    private var filenameGroup: some View {
        VStack(alignment: .leading, spacing: 8) {
            FilenameTokenEditor(
                tokens: preset.filenameTokens,
                setTokens: { tokens in
                    var changed = preset
                    changed.filenameTokens = tokens
                    update(changed)
                }
            )
            Text(
                "Preview: \(Text(previewFilename).monospaced())"
            )
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .presetEditorRowInsets()
    }

    private var scopeRow: some View {
        LabeledContent("Show in Save As… for") {
            HStack(spacing: 16) {
                Toggle("Track", isOn: appliesToTrackBinding)
                Toggle("Release", isOn: appliesToReleaseBinding)
            }
        }
        .disabled(preset.pregapPlacement == .singleFileWithCue)
        .presetEditorRowInsets()
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

    private var previewFilename: String {
        BridgeSaveFilenameToken.previewFilename(
            tokens: preset.filenameTokens,
            fileExtension: preset.codec.fileExtension
        )
    }

    private func commitName() {
        guard nameValid, nameDraft != preset.name else { return }
        var changed = preset
        changed.name = nameDraft
        update(changed)
    }

    private var appliesToTrackBinding: Binding<Bool> {
        Binding(
            get: { preset.trackScopeOn },
            set: { enabled in update(preset.withTrackScope(enabled)) }
        )
    }

    private var appliesToReleaseBinding: Binding<Bool> {
        Binding(
            get: { preset.releaseScopeOn },
            set: { enabled in update(preset.withReleaseScope(enabled)) }
        )
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
