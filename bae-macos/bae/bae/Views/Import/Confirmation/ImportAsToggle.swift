import BaeKit
import SwiftUI

/// "Import as: Exact pressing | Metadata only" segmented choice. Replaces the
/// old per-row Exact/Metadata buttons — the user picks a pressing first, then
/// chooses how to claim it here.
///
/// - **Exact pressing** — claim this pressing; its year/format/label/cat#/
///   country seed the editable fields.
/// - **Metadata only** — claim just the album group; the pressing fields stay
///   blank.
struct ImportAsToggle: View {
    let isMetadataOnly: Bool
    /// `true` selects Exact pressing, `false` selects Metadata only.
    let onSelectExactness: (Bool) -> Void

    var body: some View {
        HStack(spacing: 10) {
            Text("Import as")
                .font(.caption)
                .foregroundStyle(.secondary)
            HStack(spacing: 2) {
                segment("Exact pressing", selected: !isMetadataOnly) {
                    onSelectExactness(true)
                }
                segment("Metadata only", selected: isMetadataOnly) {
                    onSelectExactness(false)
                }
            }
            .padding(2)
            .background(Theme.field, in: RoundedRectangle(cornerRadius: 7))
        }
    }

    private func segment(
        _ title: LocalizedStringKey,
        selected: Bool,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 12, weight: selected ? .semibold : .medium))
                .foregroundStyle(selected ? Color.white : .secondary)
                .padding(.horizontal, 12)
                .padding(.vertical, 4)
                .background(
                    selected ? Theme.accent : .clear,
                    in: RoundedRectangle(cornerRadius: 5)
                )
        }
        .buttonStyle(.plain)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Exact pressing") {
        ImportAsToggle(isMetadataOnly: false, onSelectExactness: { _ in })
            .padding()
            .windowBackground()
    }

    #Preview("Metadata only") {
        VStack(alignment: .leading, spacing: 10) {
            ImportAsToggle(isMetadataOnly: true, onSelectExactness: { _ in })
            MetadataOnlyNote()
        }
        .padding()
        .frame(width: 420)
        .windowBackground()
    }
#endif

/// The pane's Exact / Metadata-only choice, present only for a source-backed
/// pick (absent for Unknown imports). Bundling the state and its handler lets
/// the confirm view take one optional instead of a `canChoose` flag plus a
/// `?? false` default.
struct ImportExactnessChoice {
    let isMetadataOnly: Bool
    /// `true` selects Exact pressing, `false` selects Metadata only.
    let onSelect: (Bool) -> Void
}
