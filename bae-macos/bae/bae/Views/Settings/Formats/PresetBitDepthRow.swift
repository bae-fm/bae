import BaeKit
import SwiftUI

/// The bit-depth row for lossless presets. It lives in the editor's Advanced
/// group — a secondary setting most presets keep at the source depth.
struct PresetBitDepthRow: View {
    let preset: BridgeSavePreset
    let update: (BridgeSavePreset) -> Void

    var body: some View {
        LabeledContent("Bit depth") {
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
                Text("Source").tag(BridgeSaveBitDepth.source)
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
                update: { _ in },
            )
        }
        .formStyle(.grouped)
        .frame(width: 460)
    }
#endif
