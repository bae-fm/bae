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
