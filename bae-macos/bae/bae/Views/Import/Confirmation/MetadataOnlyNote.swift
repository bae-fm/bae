import SwiftUI

/// The "Metadata only — pressing fields stay blank" note shown under the
/// toggle when Metadata only is chosen.
struct MetadataOnlyNote: View {
    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "info.circle")
                .foregroundStyle(.blue)
            Text(
                "Metadata only — pressing fields stay blank; just the album group is recorded."
            )
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            Color.blue.opacity(0.1),
            in: RoundedRectangle(cornerRadius: 6)
        )
    }
}

#if DEBUG
    #Preview("Metadata only note") {
        MetadataOnlyNote()
            .padding()
            .frame(width: 420)
            .windowBackground()
    }
#endif
