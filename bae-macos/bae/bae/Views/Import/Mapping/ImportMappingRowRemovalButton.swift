import BaeKit
import SwiftUI

/// The removal a track row offers: what the X is called, the sentence its
/// tooltip says about what it does, and what it does.
struct ImportMappingRowRemoval {
    let label: String
    let help: String
    let perform: () -> Void
}

/// The X at the far right of a track row that takes the row out of the list.
/// Present in the layout whether or not it is offered — the slot never
/// resizes — and hit-testable only while it is, so an unoffered X cannot be
/// pressed by accident.
struct ImportMappingRowRemovalButton: View {
    let removal: ImportMappingRowRemoval
    /// Whether the row is offering the removal right now — while the pointer
    /// is on the row.
    let offered: Bool

    /// The X's own hover, distinct from the row's: it backs the ring that
    /// marks the control as live before any press.
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
                        .fill(Theme.accent.opacity(hovering ? 0.22 : 0))
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
