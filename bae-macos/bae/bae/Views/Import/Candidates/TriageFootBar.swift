import BaeKit
import SwiftUI

/// The Ready tab's foot bar: the selection count and the bulk-import button.
/// Exists only in Ready — multi-select is the whole point of the Ready rule,
/// and nowhere else offers a checkbox.
struct TriageFootBar: View {
    let selectedCount: Int
    let readyCount: Int
    let onSelectAll: () -> Void
    let onImport: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Text(String(localized: "\(selectedCount) selected"))
                .font(.system(size: 12.5))
                .foregroundStyle(.secondary)
            Spacer()
            Button(action: onSelectAll) {
                Text(String(localized: "Select All"))
                    .font(.system(size: 12.5, weight: .medium))
                    .padding(.horizontal, 13)
                    .padding(.vertical, 6)
                    .overlay(
                        Capsule().strokeBorder(Color.secondary.opacity(0.35))
                    )
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .disabled(readyCount == 0)

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
            onImport: {}
        )
        .frame(width: 320)
        .windowBackground()
    }
#endif
