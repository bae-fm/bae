import BaeKit
import SwiftUI

/// The bitrate row for lossy presets: a pop-up of the bitrates the format's
/// encoder writes, as core lists them.
struct PresetBitrateRow: View {
    let preset: BridgeSavePreset
    let bitrate: UInt32
    let accepted: [UInt32]
    let update: (BridgeSavePreset) -> Void

    var body: some View {
        LabeledContent("Bitrate") {
            MacPopUpButton(
                "Bitrate",
                selection: Binding(
                    get: { bitrate },
                    set: { bitrate in
                        var changed = preset
                        changed.codec = preset.codec.with(bitrateKbps: bitrate)
                        update(changed)
                    }
                )
            ) {
                for kbps in accepted {
                    MacPopUpOption(
                        verbatim: String(localized: "\(Int(kbps)) kbps"),
                        value: kbps
                    )
                }
            }
        }
    }
}

#if DEBUG
    #Preview("Bitrate") {
        Form {
            PresetBitrateRow(
                preset: PreviewData.savePresets[1],
                bitrate: 320,
                accepted: [96, 128, 160, 192, 256, 320],
                update: { _ in },
            )
        }
        .formStyle(.settings)
        .frame(width: 460)
    }
#endif
