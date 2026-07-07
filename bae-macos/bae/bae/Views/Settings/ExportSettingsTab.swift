import BaeKit
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
    @State
    private var presetDrafts: [BridgeExportPreset] = []

    var body: some View {
        Form {
            Section("Release exports") {
                ExportLocationPicker(
                    configStore: configStore,
                    setLocation: exports.setExportLocation,
                    showError: { @MainActor error in uiStore.showError(error) }
                )
                Picker(
                    "Default format",
                    selection: defaultReleaseSelectionBinding()
                ) {
                    Text("Original").tag(BridgeExportSelection.original)
                    ForEach(releasePresets, id: \.id) { preset in
                        Text(preset.name)
                            .tag(
                                BridgeExportSelection.preset(
                                    presetId: preset.id
                                )
                            )
                    }
                }
            }

            Section {
                Picker(
                    "Default format",
                    selection: defaultTrackSelectionBinding()
                ) {
                    Text("Original").tag(BridgeExportSelection.original)
                    ForEach(trackPresets, id: \.id) { preset in
                        Text(preset.name)
                            .tag(
                                BridgeExportSelection.preset(
                                    presetId: preset.id
                                )
                            )
                    }
                }
                LabeledContent("Filename format") {
                    HStack(spacing: 8) {
                        TextField("Filename format", text: $templateDraft)
                            .labelsHidden()
                            .textFieldStyle(.roundedBorder)
                            .onSubmit(saveTemplate)
                        Button("Save", action: saveTemplate)
                    }
                }
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

            Section("Presets") {
                ForEach($presetDrafts, id: \.id) { $preset in
                    ExportPresetRow(preset: $preset) {
                        presetDrafts.removeAll { $0.id == preset.id }
                    }
                }
                Menu("Add preset") {
                    Button("FLAC") { addPreset(.flac) }
                    Button("MP3") { addPreset(.mp3) }
                    Button("Opus") { addPreset(.opusOgg) }
                    Button("WAV") { addPreset(.wav) }
                    Button("AIFF") { addPreset(.aiff) }
                }
                Button("Save presets", action: savePresets)
            }
        }
        .formStyle(.grouped)
        .task(id: configStore.config.exportFilenameTemplate) {
            templateDraft = configStore.config.exportFilenameTemplate
        }
        .task(id: configStore.config.exportPresets) {
            presetDrafts = configStore.config.exportPresets
        }
    }

    private var trackPresets: [BridgeExportPreset] {
        configStore.config.exportPresets.filter(\.appliesToTrack)
    }

    private var releasePresets: [BridgeExportPreset] {
        configStore.config.exportPresets.filter(\.appliesToRelease)
    }

    private func saveTemplate() {
        set { try exports.setExportFilenameTemplate(templateDraft) }
    }

    private func savePresets() {
        set { try exports.setExportPresets(presetDrafts) }
    }

    private func addPreset(_ kind: ExportPresetKind) {
        let codec = kind.codec
        presetDrafts.append(
            BridgeExportPreset(
                id: UUID().uuidString.replacingOccurrences(of: "-", with: ""),
                name: kind.label,
                codec: codec,
                extension: codec.fileExtension,
                filenameTemplate: templateDraft,
                pregapPlacement: .appendToPreviousExceptHtoa,
                appliesToTrack: true,
                appliesToRelease: true
            )
        )
    }

    private func defaultTrackSelectionBinding() -> Binding<
        BridgeExportSelection
    > {
        Binding(
            get: { configStore.config.defaultTrackExportSelection },
            set: { selection in
                set { try exports.setDefaultTrackExportSelection(selection) }
            }
        )
    }

    private func defaultReleaseSelectionBinding()
        -> Binding<BridgeExportSelection>
    {
        Binding(
            get: { configStore.config.defaultReleaseExportSelection },
            set: { selection in
                set { try exports.setDefaultReleaseExportSelection(selection) }
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

private struct ExportPresetRow: View {
    @Binding
    var preset: BridgeExportPreset
    var delete: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            TextField("Name", text: $preset.name)
            TextField("Filename format", text: $preset.filenameTemplate)
            HStack {
                Text(preset.codec.label)
                Spacer()
                Toggle("Track", isOn: appliesToTrackBinding)
                    .disabled(preset.pregapPlacement == .singleFileWithCue)
                Toggle("Release", isOn: appliesToReleaseBinding)
                    .disabled(preset.pregapPlacement == .singleFileWithCue)
            }
            PresetCodecEditor(preset: $preset)
            Picker("Pregap", selection: pregapPlacementBinding) {
                Text("Append except HTOA")
                    .tag(
                        BridgeExportPregapPlacement.appendToPreviousExceptHtoa
                    )
                Text("Append including HTOA")
                    .tag(
                        BridgeExportPregapPlacement
                            .appendToPreviousIncludingHtoa
                    )
                Text("Exclude")
                    .tag(BridgeExportPregapPlacement.exclude)
                Text("Single file + CUE")
                    .tag(BridgeExportPregapPlacement.singleFileWithCue)
                    .disabled(!preset.codec.supportsSingleFileCue)
            }
            HStack {
                Button("Delete", role: .destructive, action: delete)
            }
        }
    }

    private var appliesToTrackBinding: Binding<Bool> {
        Binding(
            get: {
                preset.pregapPlacement != .singleFileWithCue
                    && preset.appliesToTrack
            },
            set: { enabled in
                preset.appliesToTrack =
                    enabled && preset.pregapPlacement != .singleFileWithCue
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
                preset.appliesToRelease =
                    enabled || preset.pregapPlacement == .singleFileWithCue
            }
        )
    }

    private var pregapPlacementBinding: Binding<BridgeExportPregapPlacement> {
        Binding(
            get: { preset.pregapPlacement },
            set: { placement in
                preset.pregapPlacement = placement
                if placement == .singleFileWithCue {
                    preset.appliesToTrack = false
                    preset.appliesToRelease = true
                }
            }
        )
    }
}

extension BridgeExportPresetCodec {
    fileprivate var supportsSingleFileCue: Bool {
        switch self {
        case .opusOgg:
            false
        case .flac, .mp3, .wav, .aiff:
            true
        }
    }
}

private struct PresetCodecEditor: View {
    @Binding
    var preset: BridgeExportPreset

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
            Picker("Bit depth", selection: bitDepthBinding) {
                Text("Source").tag(BridgeExportBitDepth.source)
                Text("16-bit").tag(BridgeExportBitDepth.bits16)
                Text("24-bit").tag(BridgeExportBitDepth.bits24)
                Text("32-bit").tag(BridgeExportBitDepth.bits32)
            }
        case .mp3, .opusOgg:
            LabeledContent("Bitrate") {
                TextField(
                    "Bitrate",
                    value: bitrateBinding,
                    formatter: Self.bitrateFormatter
                )
                .labelsHidden()
                .textFieldStyle(.roundedBorder)
                .frame(width: 96)
            }
        }
    }

    private var bitDepthBinding: Binding<BridgeExportBitDepth> {
        Binding(
            get: {
                switch preset.codec {
                case .flac(let bitDepth),
                    .wav(let bitDepth),
                    .aiff(let bitDepth):
                    bitDepth
                case .mp3, .opusOgg:
                    .source
                }
            },
            set: { bitDepth in
                switch preset.codec {
                case .flac:
                    preset.codec = .flac(bitDepth: bitDepth)
                    preset.extension = preset.codec.fileExtension
                case .wav:
                    preset.codec = .wav(bitDepth: bitDepth)
                    preset.extension = preset.codec.fileExtension
                case .aiff:
                    preset.codec = .aiff(bitDepth: bitDepth)
                    preset.extension = preset.codec.fileExtension
                case .mp3, .opusOgg:
                    break
                }
            }
        )
    }

    private var bitrateBinding: Binding<UInt32> {
        Binding(
            get: {
                switch preset.codec {
                case .mp3(let bitrateKbps),
                    .opusOgg(let bitrateKbps):
                    bitrateKbps
                case .flac, .wav, .aiff:
                    0
                }
            },
            set: { bitrateKbps in
                switch preset.codec {
                case .mp3:
                    preset.codec = .mp3(bitrateKbps: bitrateKbps)
                    preset.extension = preset.codec.fileExtension
                case .opusOgg:
                    preset.codec = .opusOgg(bitrateKbps: bitrateKbps)
                    preset.extension = preset.codec.fileExtension
                case .flac, .wav, .aiff:
                    break
                }
            }
        )
    }
}

private enum ExportPresetKind {
    case flac
    case mp3
    case opusOgg
    case wav
    case aiff

    var label: String {
        switch self {
        case .flac: "FLAC"
        case .mp3: "MP3"
        case .opusOgg: "Opus"
        case .wav: "WAV"
        case .aiff: "AIFF"
        }
    }

    var codec: BridgeExportPresetCodec {
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
    fileprivate var label: String {
        switch self {
        case .flac: "FLAC"
        case .mp3(let bitrateKbps): "MP3 \(bitrateKbps) kbps"
        case .opusOgg(let bitrateKbps): "Opus \(bitrateKbps) kbps"
        case .wav: "WAV"
        case .aiff: "AIFF"
        }
    }

    fileprivate var fileExtension: String {
        switch self {
        case .flac: "flac"
        case .mp3: "mp3"
        case .opusOgg: "ogg"
        case .wav: "wav"
        case .aiff: "aiff"
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
