import BaeKit
import SwiftUI

/// Pending's foot bar: the selection count and the bulk-import button. Only
/// rows core marks selectable contribute to it.
struct TriageFootBar: View {
    let selectedCount: Int
    let readyCount: Int
    let onSelectAll: () -> Void
    let onSelectNone: () -> Void
    let onImport: () -> Void

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
            Button(action: onImport) {
                Text(String(localized: "Import \(selectedCount)"))
                    .font(.system(size: 12.5, weight: .semibold))
                    .padding(.horizontal, 15)
                    .padding(.vertical, 6)
                    .foregroundStyle(Theme.background)
                    .background(Capsule().fill(Theme.accent))
            }
            .buttonStyle(.plain)
            .disabled(selectedCount == 0)
            .opacity(selectedCount == 0 ? 0.5 : 1)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 11)
        .background(
            LinearGradient(
                colors: [Theme.surfaceElevated, Theme.surface],
                startPoint: .top,
                endPoint: .bottom
            )
        )
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Foot bar") {
        TriageFootBar(
            selectedCount: 3,
            readyCount: 18,
            onSelectAll: {},
            onSelectNone: {},
            onImport: {}
        )
        .frame(width: 320)
        .windowBackground()
    }
#endif
