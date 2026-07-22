import BaeKit
import SwiftUI

/// One export preset as a disclosure row: collapsed, the name, a settings
/// summary, and which export menus offer it; expanded, the full editor. Every
/// control writes a whole updated preset up through `update` — the caller
/// saves the preset set and the change round-trips via `configChanged`.
struct ExportPresetEditor: View {
    let preset: BridgeExportPreset
    @Binding
    var isExpanded: Bool
    let update: (BridgeExportPreset) -> Void
    let delete: () -> Void

    /// Editable copy of the name, committed on submit or focus loss so
    /// mid-edit keystrokes don't churn config writes.
    @State
    private var nameDraft = ""
    @FocusState
    private var nameFocused: Bool
    @State
    private var confirmingDelete = false

    var body: some View {
        DisclosureGroup(isExpanded: $isExpanded) {
            nameRow
            formatRow
            PresetCodecEditor(preset: preset, update: update)
            pregapRow
            filenameRows
            scopeRow
            deleteRow
        } label: {
            ExportPresetSummaryRow(preset: preset)
        }
        .task(id: preset.name) {
            nameDraft = preset.name
        }
    }

    private var nameRow: some View {
        TextField("Name", text: $nameDraft)
            .focused($nameFocused)
            .onSubmit(commitName)
            .onChange(of: nameFocused) { _, focused in
                if !focused {
                    commitName()
                }
            }
    }

    private var formatRow: some View {
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
    }

    private var pregapRow: some View {
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
    }

    @ViewBuilder
    private var filenameRows: some View {
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
    }

    private var scopeRow: some View {
        LabeledContent("Show in export menu for") {
            HStack(spacing: 16) {
                Toggle("Track", isOn: appliesToTrackBinding)
                Toggle("Release", isOn: appliesToReleaseBinding)
            }
        }
        .disabled(preset.pregapPlacement == .singleFileWithCue)
    }

    private var deleteRow: some View {
        Button("Delete preset…", role: .destructive) {
            confirmingDelete = true
        }
        .confirmationDialog(
            "Delete the “\(preset.name)” preset?",
            isPresented: $confirmingDelete
        ) {
            Button("Delete", role: .destructive, action: delete)
        }
    }

    private var previewFilename: String {
        BridgeExportFilenameToken.previewFilename(
            tokens: preset.filenameTokens,
            fileExtension: preset.codec.fileExtension
        )
    }

    private func commitName() {
        guard nameDraft != preset.name else { return }
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

/// The collapsed row: name over a settings summary, with the export menus the
/// preset appears in as trailing badges.
private struct ExportPresetSummaryRow: View {
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
/// bit depth, lossy ones a bitrate.
private struct PresetCodecEditor: View {
    let preset: BridgeExportPreset
    let update: (BridgeExportPreset) -> Void

    private static let bitrateFormatter: NumberFormatter = {
        let formatter = NumberFormatter()
        formatter.numberStyle = .none
        formatter.minimum = 1
        formatter.maximum = 512
        return formatter
    }()

    var body: some View {
        switch preset.codec {
        case .flac, .wav, .aiff:
            Picker(
                "Bit depth",
                selection: Binding(
                    get: { preset.codec.bitDepth ?? .source },
                    set: { bitDepth in
                        var changed = preset
                        changed.codec = preset.codec.with(bitDepth: bitDepth)
                        update(changed)
                    }
                )
            ) {
                Text("Source").tag(BridgeExportBitDepth.source)
                Text("16-bit").tag(BridgeExportBitDepth.bits16)
                Text("24-bit").tag(BridgeExportBitDepth.bits24)
                Text("32-bit").tag(BridgeExportBitDepth.bits32)
            }
        case .mp3, .opusOgg:
            LabeledContent("Bitrate") {
                TextField(
                    "Bitrate",
                    value: Binding(
                        get: { preset.codec.bitrateKbps ?? 0 },
                        set: { bitrateKbps in
                            var changed = preset
                            changed.codec = preset.codec.with(
                                bitrateKbps: bitrateKbps
                            )
                            update(changed)
                        }
                    ),
                    formatter: Self.bitrateFormatter
                )
                .labelsHidden()
                .textFieldStyle(.roundedBorder)
                .frame(width: 96)
            }
        }
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
