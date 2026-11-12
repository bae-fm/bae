import BaeKit
import SwiftUI

/// The bit-depth row for lossless presets: the source's depth, or 16-bit.
struct PresetBitDepthRow: View {
    let preset: BridgeSavePreset
    let bitDepth: BridgeSaveBitDepth
    let update: (BridgeSavePreset) -> Void

    var body: some View {
        LabeledContent("Bit depth") {
            MacPopUpButton(
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
                MacPopUpOption(
                    verbatim: BridgeSaveBitDepth.sameAsSource.label,
                    value: BridgeSaveBitDepth.sameAsSource
                )
                MacPopUpOption(
                    verbatim: BridgeSaveBitDepth.bits16.label,
                    value: BridgeSaveBitDepth.bits16
                )
            }
        }
    }
}
