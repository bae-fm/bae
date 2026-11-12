import BaeKit
import SwiftUI

/// A preset's label in the settings list: its name.
struct SavePresetSummaryRow: View {
    let preset: BridgeSavePreset

    var body: some View {
        HStack(alignment: .center) {
            Text(preset.name)
                .themeText(.body)
            Spacer()
        }
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
        .formStyle(.settings)
        .frame(width: 460)
    }
#endif
