import BaeKit
import SwiftUI

/// The bit-depth row for lossless presets, in the editor's Advanced group.
/// Every lossless preset writes one of the three depths it lists.
struct PresetBitDepthRow: View {
    let preset: BridgeSavePreset
    let bitDepth: BridgeSaveBitDepth
    let update: (BridgeSavePreset) -> Void

    var body: some View {
        LabeledContent("Bit depth") {
            Picker(
                "Bit depth",
                selection: Binding(
                    get: { bitDepth },
                    set: { bitDepth in
                        var changed = preset
                        changed.codec = preset.codec.with(bitDepth: bitDepth)
                        update(changed)
                    }
                )
            ) {
                Text("16-bit").tag(BridgeSaveBitDepth.bits16)
                Text("24-bit").tag(BridgeSaveBitDepth.bits24)
                Text("32-bit").tag(BridgeSaveBitDepth.bits32)
            }
            .labelsHidden()
        }
        .presetEditorRowInsets()
    }
}

#if DEBUG
    #Preview("Bit depth") {
        Form {
            PresetBitDepthRow(
                preset: PreviewData.savePresets[0],
                bitDepth: .bits24,
                update: { _ in },
            )
        }
        .formStyle(.grouped)
        .frame(width: 460)
    }
#endif
