import BaeKit
import SwiftUI

/// Pending's foot bar: the selection count and the bulk-import buttons. Only
/// rows core marks selectable contribute to it. Import identified takes the
/// selected rows whose draft was read from a catalog's release; Import ready
/// takes every selected Ready row, tag drafts included.
struct TriageFootBar: View {
    let selectedCount: Int
    let selectedIdentifiedCount: Int
    let readyCount: Int
    let onSelectAll: () -> Void
    let onSelectNone: () -> Void
    let onImport: () -> Void
    let onImportIdentified: () -> Void

    /// Every selectable row is already selected, so the control has nothing
    /// left to add and becomes the way to clear.
    private var allSelected: Bool {
        readyCount > 0 && selectedCount >= readyCount
    }

    var body: some View {
        HStack(spacing: 8) {
            Button(action: allSelected ? onSelectNone : onSelectAll) {
                Text(
                    allSelected
                        ? String(localized: "Select None")
                        : String(localized: "Select All")
                )
                .font(.system(size: 12))
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .disabled(readyCount == 0)
            Spacer()
            importButton(
                String(
                    localized:
                        "\(String(localized: "Import identified")) (\(selectedIdentifiedCount))"
                ),
                count: selectedIdentifiedCount,
                action: onImportIdentified
            )
            importButton(
                BridgeCandidateAction.importReady.label(count: selectedCount),
                count: selectedCount,
                action: onImport
            )
        }
        .padding(.horizontal, ImportListHierarchyLayout.rowEdgePadding)
        .padding(.vertical, 12)
        .background(Theme.surface)
    }

    private func importButton(
        _ label: String,
        count: Int,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Text(label)
                .font(.system(size: 12.5, weight: .semibold))
                .foregroundStyle(
                    count == 0
                        ? AnyShapeStyle(.tertiary)
                        : AnyShapeStyle(Theme.accent)
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(count == 0)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Foot bar") {
        TriageFootBar(
            selectedCount: 3,
            selectedIdentifiedCount: 2,
            readyCount: 18,
            onSelectAll: {},
            onSelectNone: {},
            onImport: {},
            onImportIdentified: {}
        )
        .frame(width: 320)
        .windowBackground()
    }
#endif
