import BaeKit
import SwiftUI

/// The edit sheet for one export preset. Every control writes a whole updated
/// preset up through `update` — the caller saves the preset set and the change
/// round-trips via `configChanged`, so the sheet always renders the stored
/// state and Done only closes it. Deleting lives on the preset's list row,
/// not here.
struct ExportPresetEditor: View {
    let preset: BridgeExportPreset
    let update: (BridgeExportPreset) -> Void

    @Environment(\.dismiss)
    private var dismiss

    /// Editable copies of the free-form fields, committed on submit or focus
    /// loss so mid-edit keystrokes don't churn config writes. Invalid drafts
    /// never commit; they disable Done instead.
    @State
    private var nameDraft = ""
    @State
    private var bitrateDraft = ""
    @FocusState
    private var nameFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            Form {
                nameRow
                formatRow
                PresetCodecEditor(
                    preset: preset,
                    bitrateDraft: $bitrateDraft,
                    update: update
                )
                pregapRow
                filenameGroup
                scopeRow
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
                    if changed.pregapPlacement == .singleFileWithCue,
                        !changed.codec.supportsSingleFileCue
                    {
                        changed.pregapPlacement = .appendToPreviousExceptHtoa
                    }
                    update(changed)
                }
            )
        ) {
            ForEach(ExportPresetKind.allCases, id: \.self) { kind in
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
                .tag(BridgeExportPregapPlacement.appendToPreviousExceptHtoa)
            Text("Append including HTOA")
                .tag(BridgeExportPregapPlacement.appendToPreviousIncludingHtoa)
            Text("Exclude")
                .tag(BridgeExportPregapPlacement.exclude)
            if preset.codec.supportsSingleFileCue {
                Text("Single file + CUE")
                    .tag(BridgeExportPregapPlacement.singleFileWithCue)
            }
        }
        .labelsHidden()
    }

    private var filenameGroup: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Filename format")
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
        LabeledContent("Show in export menu for") {
            HStack(spacing: 16) {
                Toggle("Track", isOn: appliesToTrackBinding)
                Toggle("Release", isOn: appliesToReleaseBinding)
            }
        }
        .disabled(preset.pregapPlacement == .singleFileWithCue)
        .presetEditorRowInsets()
    }

    private var previewFilename: String {
        BridgeExportFilenameToken.previewFilename(
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
            get: {
                preset.pregapPlacement != .singleFileWithCue
                    && preset.appliesToTrack
            },
            set: { enabled in
                var changed = preset
                changed.appliesToTrack =
                    enabled && preset.pregapPlacement != .singleFileWithCue
                update(changed)
            }
        )
    }

    private var appliesToReleaseBinding: Binding<Bool> {
        Binding(
            get: {
                preset.pregapPlacement == .singleFileWithCue
                    || preset.appliesToRelease
            },
            set: { enabled in
                var changed = preset
                changed.appliesToRelease =
                    enabled || preset.pregapPlacement == .singleFileWithCue
                update(changed)
            }
        )
    }
}

extension View {
    /// The edit sheet's card padding: how far every grouped-form row's
    /// content sits from the card's edges, on top of the platform's own row
    /// insets. (`listRowInsets` is ignored by macOS grouped forms, so this
    /// pads the row content itself.)
    fileprivate func presetEditorRowInsets() -> some View {
        padding(EdgeInsets(top: 2, leading: 8, bottom: 0, trailing: 8))
    }
}

/// A preset's list row in settings: name over a settings summary, with the
/// export menus the preset appears in as trailing badges. Clicking the row
/// opens `ExportPresetEditor`.
struct ExportPresetSummaryRow: View {
    let preset: BridgeExportPreset

    var body: some View {
        HStack(alignment: .center) {
            VStack(alignment: .leading, spacing: 2) {
                Text(preset.name)
                    .fontWeight(.semibold)
                Text(summary)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if preset.appliesToTrack {
                ScopeBadge(label: Text("Track"))
            }
            if preset.appliesToRelease {
                ScopeBadge(label: Text("Release"))
            }
        }
    }

    private var summary: String {
        var parts = [preset.codec.label]
        if let bitDepth = preset.codec.bitDepth {
            parts.append(bitDepth.summaryLabel)
        }
        parts.append(preset.pregapPlacement.label)
        return parts.joined(separator: " · ")
    }
}

private struct ScopeBadge: View {
    let label: Text

    var body: some View {
        label
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 8)
            .padding(.vertical, 1)
            .background(Capsule().fill(.quaternary))
    }
}

/// The bit depth or bitrate row, per the codec family: lossless codecs carry a
/// bit depth, lossy ones a bitrate. The bitrate edits through the sheet-owned
/// draft; only in-range values commit.
private struct PresetCodecEditor: View {
    let preset: BridgeExportPreset
    @Binding
    var bitrateDraft: String
    let update: (BridgeExportPreset) -> Void

    @FocusState
    private var bitrateFocused: Bool

    var body: some View {
        switch preset.codec {
        case .flac, .wav, .aiff:
            LabeledContent("Bit depth") {
                Picker(
                    "Bit depth",
                    selection: Binding(
                        get: { preset.codec.bitDepth ?? .source },
                        set: { bitDepth in
                            var changed = preset
                            changed.codec = preset.codec.with(
                                bitDepth: bitDepth
                            )
                            update(changed)
                        }
                    )
                ) {
                    Text("Source").tag(BridgeExportBitDepth.source)
                    Text("16-bit").tag(BridgeExportBitDepth.bits16)
                    Text("24-bit").tag(BridgeExportBitDepth.bits24)
                    Text("32-bit").tag(BridgeExportBitDepth.bits32)
                }
                .labelsHidden()
            }
            .presetEditorRowInsets()
        case .mp3, .opusOgg:
            LabeledContent("Bitrate") {
                TextField("Bitrate", text: $bitrateDraft)
                    .labelsHidden()
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 96)
                    .focused($bitrateFocused)
                    .onSubmit(commitBitrate)
                    .onChange(of: bitrateFocused) { _, focused in
                        if !focused {
                            commitBitrate()
                        }
                    }
            }
            .presetEditorRowInsets()
        }
    }

    private func commitBitrate() {
        guard let range = preset.codec.kind.bitrateRange,
            let value = UInt32(bitrateDraft),
            range.contains(value),
            value != preset.codec.bitrateKbps
        else { return }
        var changed = preset
        changed.codec = preset.codec.with(bitrateKbps: value)
        update(changed)
    }
}

/// The codec families the Format picker and the add-preset menu offer.
enum ExportPresetKind: CaseIterable {
    case flac
    case mp3
    case opusOgg
    case wav
    case aiff

    /// Format names are proper nouns, shown as-is in every locale.
    var label: String {
        switch self {
        case .flac: "FLAC"
        case .mp3: "MP3"
        case .opusOgg: "Opus"
        case .wav: "WAV"
        case .aiff: "AIFF"
        }
    }

    var defaultCodec: BridgeExportPresetCodec {
        switch self {
        case .flac: .flac(bitDepth: .source)
        case .mp3: .mp3(bitrateKbps: 320)
        case .opusOgg: .opusOgg(bitrateKbps: 192)
        case .wav: .wav(bitDepth: .source)
        case .aiff: .aiff(bitDepth: .source)
        }
    }

    /// The bitrate range core's preset validation accepts for the lossy
    /// families; nil for lossless, which carry no bitrate.
    var bitrateRange: ClosedRange<UInt32>? {
        switch self {
        case .mp3: 32...320
        case .opusOgg: 32...512
        case .flac, .wav, .aiff: nil
        }
    }
}

extension BridgeExportPresetCodec {
    var kind: ExportPresetKind {
        switch self {
        case .flac: .flac
        case .mp3: .mp3
        case .opusOgg: .opusOgg
        case .wav: .wav
        case .aiff: .aiff
        }
    }

    /// Switch codec family, carrying the parameter that still applies: bit
    /// depth across lossless codecs, bitrate across lossy ones (clamped into
    /// MP3's supported range). A cross-family switch takes the family default.
    func switched(to kind: ExportPresetKind) -> BridgeExportPresetCodec {
        switch kind {
        case .flac: .flac(bitDepth: bitDepth ?? .source)
        case .wav: .wav(bitDepth: bitDepth ?? .source)
        case .aiff: .aiff(bitDepth: bitDepth ?? .source)
        case .mp3: .mp3(bitrateKbps: min(max(bitrateKbps ?? 320, 32), 320))
        case .opusOgg: .opusOgg(bitrateKbps: bitrateKbps ?? 192)
        }
    }

    /// The lossless family's bit depth; nil for lossy codecs.
    var bitDepth: BridgeExportBitDepth? {
        switch self {
        case .flac(let bitDepth), .wav(let bitDepth), .aiff(let bitDepth):
            bitDepth
        case .mp3, .opusOgg:
            nil
        }
    }

    /// The lossy family's bitrate; nil for lossless codecs.
    var bitrateKbps: UInt32? {
        switch self {
        case .mp3(let bitrateKbps), .opusOgg(let bitrateKbps):
            bitrateKbps
        case .flac, .wav, .aiff:
            nil
        }
    }

    func with(bitDepth: BridgeExportBitDepth) -> BridgeExportPresetCodec {
        switch self {
        case .flac: .flac(bitDepth: bitDepth)
        case .wav: .wav(bitDepth: bitDepth)
        case .aiff: .aiff(bitDepth: bitDepth)
        case .mp3, .opusOgg: self
        }
    }

    func with(bitrateKbps: UInt32) -> BridgeExportPresetCodec {
        switch self {
        case .mp3: .mp3(bitrateKbps: bitrateKbps)
        case .opusOgg: .opusOgg(bitrateKbps: bitrateKbps)
        case .flac, .wav, .aiff: self
        }
    }

    var supportsSingleFileCue: Bool {
        switch self {
        case .opusOgg:
            false
        case .flac, .mp3, .wav, .aiff:
            true
        }
    }

    var label: String {
        switch self {
        case .flac: "FLAC"
        case .mp3(let bitrateKbps):
            String(localized: "MP3 \(Int(bitrateKbps)) kbps")
        case .opusOgg(let bitrateKbps):
            String(localized: "Opus \(Int(bitrateKbps)) kbps")
        case .wav: "WAV"
        case .aiff: "AIFF"
        }
    }

    var fileExtension: String {
        switch self {
        case .flac: "flac"
        case .mp3: "mp3"
        case .opusOgg: "ogg"
        case .wav: "wav"
        case .aiff: "aiff"
        }
    }
}

extension BridgeExportBitDepth {
    /// The summary line's bit-depth part: "Source" alone reads as nothing in a
    /// "FLAC · Source · …" join, so the source case names what it is.
    var summaryLabel: String {
        switch self {
        case .source: String(localized: "Source bit depth")
        case .bits16: String(localized: "16-bit")
        case .bits24: String(localized: "24-bit")
        case .bits32: String(localized: "32-bit")
        }
    }
}

extension BridgeExportPregapPlacement {
    var label: String {
        switch self {
        case .appendToPreviousExceptHtoa:
            String(localized: "Append except HTOA")
        case .appendToPreviousIncludingHtoa:
            String(localized: "Append including HTOA")
        case .exclude:
            String(localized: "Exclude")
        case .singleFileWithCue:
            String(localized: "Single file + CUE")
        }
    }
}

#if DEBUG
    #Preview("Export preset editor") {
        @Previewable
        @State
        var preset = PreviewData.exportPresets[0]

        ExportPresetEditor(
            preset: preset,
            update: { preset = $0 }
        )
    }
#endif
