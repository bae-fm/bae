import BaeKit
import SwiftUI

/// A track row's removal: its label, tooltip, and action.
struct ImportMappingRowRemoval {
    let label: String
    let help: String
    let perform: () -> Void
}

/// The X at a track row's far right that removes the row. It always keeps its
/// slot but can be pressed only while offered.
struct ImportMappingRowRemovalButton: View {
    let removal: ImportMappingRowRemoval
    /// Whether the row offers the removal now, while the pointer is on it.
    let offered: Bool

    /// The X's own hover, which draws its highlight.
    @State
    private var hovering = false

    var body: some View {
        Button(action: removal.perform) {
            Image(systemName: "xmark")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(
                    hovering
                        ? AnyShapeStyle(Theme.accent)
                        : AnyShapeStyle(.tertiary)
                )
                .frame(
                    width: ImportMappingColumns.action,
                    height: ImportMappingColumns.action
                )
                .background(
                    RoundedRectangle(cornerRadius: 6)
                        .fill(hovering ? Theme.accentStrong : Color.clear)
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(PressableIconButtonStyle())
        .onHover { hovering = $0 }
        .help(removal.help)
        .accessibilityLabel(removal.label)
        .opacity(offered ? 1 : 0)
        .allowsHitTesting(offered)
    }
}
