import SwiftUI

/// A transparent ghost pill button — the toolbar's escape actions.
struct GhostPill: View {
    let icon: String?
    let label: Text
    let action: () -> Void

    init(
        icon: String?,
        label: LocalizedStringKey,
        action: @escaping () -> Void
    ) {
        self.icon = icon
        self.label = Text(label)
        self.action = action
    }

    init(
        icon: String?,
        verbatimLabel: String,
        action: @escaping () -> Void
    ) {
        self.icon = icon
        label = Text(verbatim: verbatimLabel)
        self.action = action
    }

    @State
    private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                if let icon {
                    Image(systemName: icon)
                        .font(.system(size: 11))
                }
                label
                    .font(.system(size: 12.5, weight: .medium))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .foregroundStyle(hovering ? .primary : .secondary)
            .background(
                .white.opacity(hovering ? 0.05 : 0),
                in: RoundedRectangle(cornerRadius: 7)
            )
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Ghost pills") {
        HStack(spacing: 8) {
            GhostPill(
                icon: "magnifyingglass",
                label: "Search manually",
                action: {}
            )
            GhostPill(icon: nil, label: "Skip identifying", action: {})
        }
        .padding()
        .windowBackground()
    }
#endif
