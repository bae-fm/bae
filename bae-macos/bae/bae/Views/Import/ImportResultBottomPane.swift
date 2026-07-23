import AppKit
import BaeKit
import SwiftUI

/// Sizing for the docked confirm pane, shared between the pane (drag clamp) and
/// its container (frame height) so they never disagree. Non-generic so the
/// container can call `clamp` without pinning the pane's `Content` type.
enum ImportPaneLayout {
    static let minHeight: CGFloat = 220
    static let maxHeight: CGFloat = 500
    /// Keep at least this much of the results list visible above the pane.
    private static let resultsFloor: CGFloat = 140

    /// Clamp a requested height to [min, capped-max], where the cap also
    /// reserves the results floor.
    static func clamp(_ requested: CGFloat, available: CGFloat) -> CGFloat {
        let cap = max(minHeight, min(maxHeight, available - resultsFloor))
        return max(minHeight, min(cap, requested))
    }
}

/// Docked, draggable confirm pane that slides up over the still-visible
/// results list. The drag handle resizes it (220–500pt, capped to keep the
/// results readable above); the header carries the title and a close button;
/// the body hosts the confirmation content — a loading spinner while the
/// release detail loads, then the editable confirm form.
///
/// Height and drag state live in the parent (it owns the results-vs-pane
/// split); this view drives them through bindings and reports the available
/// container height so the resize cap can leave the list a readable floor.
struct ImportResultBottomPane<Content: View>: View {
    @Binding
    var height: CGFloat
    @Binding
    var dragging: Bool
    /// Height of the pane's container, so the resize cap leaves the results
    /// list above a readable minimum.
    let available: CGFloat
    let onClose: () -> Void
    @ViewBuilder
    var content: () -> Content

    @State
    private var dragStartHeight: CGFloat?

    var body: some View {
        VStack(spacing: 0) {
            dragHandle
            header
            content()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Theme.surface)
        .overlay(alignment: .top) {
            Rectangle().fill(.white.opacity(0.1)).frame(height: 1)
        }
        .shadow(color: .black.opacity(0.45), radius: 18, y: -8)
    }

    private var dragHandle: some View {
        ZStack {
            Color.clear
            Capsule()
                .fill(dragging ? Theme.accent : Color.white.opacity(0.16))
                .frame(width: 36, height: 4)
        }
        .frame(maxWidth: .infinity)
        .frame(height: 16)
        .contentShape(Rectangle())
        .onHover { hovering in
            if hovering {
                NSCursor.resizeUpDown.push()
            }
            else {
                NSCursor.pop()
            }
        }
        .gesture(
            DragGesture(coordinateSpace: .global)
                .onChanged { value in
                    let start: CGFloat
                    if let existing = dragStartHeight {
                        start = existing
                    }
                    else {
                        start = height
                        dragStartHeight = height
                        dragging = true
                    }
                    // Drag up grows the pane.
                    height = ImportPaneLayout.clamp(
                        start - value.translation.height,
                        available: available
                    )
                }
                .onEnded { _ in
                    dragStartHeight = nil
                    dragging = false
                }
        )
        .help("Drag to resize")
    }

    private var header: some View {
        HStack(spacing: 10) {
            Text("Confirm import")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(.primary)
            Spacer()
            Button(action: onClose) {
                Image(systemName: "xmark")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: 24, height: 24)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Close")
        }
        .padding(.horizontal, 16)
        .padding(.bottom, 9)
        .overlay(alignment: .bottom) {
            Rectangle().fill(.white.opacity(0.07)).frame(height: 1)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Docked pane") {
        ImportResultBottomPane(
            height: .constant(320),
            dragging: .constant(false),
            available: 800,
            onClose: {},
        ) {
            VStack {
                Spacer()
                Text("Confirmation content")
                    .foregroundStyle(.secondary)
                Spacer()
            }
            .frame(maxWidth: .infinity)
        }
        .frame(width: 700, height: 360)
        .windowBackground()
    }
#endif
