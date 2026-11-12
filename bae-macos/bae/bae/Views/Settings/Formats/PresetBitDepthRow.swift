import BaeKit
import SwiftUI

/// The bit-depth row for lossless presets: the source's depth, or 16-bit.
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
                Text(BridgeSaveBitDepth.sameAsSource.label)
                    .tag(BridgeSaveBitDepth.sameAsSource)
                Text(BridgeSaveBitDepth.bits16.label)
                    .tag(BridgeSaveBitDepth.bits16)
            }
            .labelsHidden()
        }
    }
}

#if DEBUG
    #Preview("Bit depth") {
        Form {
            PresetBitDepthRow(
                preset: PreviewData.savePresets[0],
                bitDepth: .sameAsSource,
                update: { _ in },
            )
        }
        .formStyle(.settings)
        .frame(width: 460)
    }
#endif
