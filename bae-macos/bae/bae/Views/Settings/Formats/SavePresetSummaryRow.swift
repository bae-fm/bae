import BaeKit
import SwiftUI

/// A preset's label in the settings list: its name over its codec.
struct SavePresetSummaryRow: View {
    let preset: BridgeSavePreset

    var body: some View {
        HStack(alignment: .center) {
            VStack(alignment: .leading, spacing: 2) {
                Text(preset.name)
                    .themeText(.rowTitle)
                Text(summary)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
    }

    private var summary: String {
        preset.codec.label
    }
}

#if DEBUG
    #Preview("Preset rows") {
        Form {
            Section {
                ForEach(PreviewData.savePresets, id: \.id) { preset in
                    SavePresetSummaryRow(preset: preset)
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 460)
    }
#endif
