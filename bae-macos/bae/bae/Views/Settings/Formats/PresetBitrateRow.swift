import BaeKit
import SwiftUI

/// The bitrate row for lossy presets — the primary quality setting, so it sits
/// in the main form. The bitrate edits through the sheet-owned draft; only
/// in-range values commit.
struct PresetBitrateRow: View {
    let preset: BridgeSavePreset
    @Binding
    var bitrateDraft: String
    let update: (BridgeSavePreset) -> Void

    @FocusState
    private var bitrateFocused: Bool

    var body: some View {
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

#if DEBUG
    // MARK: - Previews

    /// Owns the bitrate draft the editor's sheet normally holds, seeded from the
    /// lossy preset fixture.
    private struct PresetBitrateRowPreview: View {
        @State
        private var draft = "320"

        var body: some View {
            Form {
                PresetBitrateRow(
                    preset: PreviewData.savePresets[1],
                    bitrateDraft: $draft,
                    update: { _ in },
                )
            }
            .formStyle(.grouped)
            .frame(width: 460)
        }
    }

    #Preview("Bitrate") {
        PresetBitrateRowPreview()
    }
#endif
