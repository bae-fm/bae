import BaeKit
import SwiftUI

/// A preset's list row in settings: name over a codec summary. The Track and
/// Release scopes it appears under are separate inline toggles in the list row,
/// not part of this label. Clicking the row opens `SavePresetEditor`.
struct SavePresetSummaryRow: View {
    let preset: BridgeSavePreset

    var body: some View {
        HStack(alignment: .center) {
            VStack(alignment: .leading, spacing: 2) {
                Text(preset.name)
                    .fontWeight(.semibold)
                Text(summary)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
    }

    private var summary: String {
        preset.codec.label
    }
}
