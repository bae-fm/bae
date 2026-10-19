import SwiftUI

/// The row of the track a pending reveal names, as that row reports itself to
/// the grid that scrolls to it: the row exists only once the album's detail is
/// laid out, which the grid cannot see from its own slots.
struct RevealedTrackRow: Equatable {
    let trackId: String
    /// The reveal the row answers, so a row laid out for an earlier one is
    /// never taken for it.
    let seq: Int
    let bounds: Anchor<CGRect>
}

extension EnvironmentValues {
    /// Takes whether the row a pending reveal names sits in full view, as the
    /// row measures itself, with the reveal's `seq`. The row's own geometry
    /// follows a programmatic scroll in the layout that applies it, where the
    /// grid's scroll measurements and a scroll's completion can come before
    /// the scroll view has applied it. Outside a grid, nothing scrolls to the
    /// row.
    @Entry
    var revealedTrackRowInView:
        @MainActor (_ seq: Int, _ inView: Bool) -> Void =
            { _, _ in }
}

struct RevealedTrackRowKey: PreferenceKey {
    static let defaultValue: RevealedTrackRow? = nil

    static func reduce(
        value: inout RevealedTrackRow?,
        nextValue: () -> RevealedTrackRow?
    ) {
        value = value ?? nextValue()
    }
}

extension View {
    /// Marks this view as `trackId`'s row, which a reveal naming the track
    /// scrolls into view and then flashes.
    func revealsAsTrackRow(_ trackId: String) -> some View {
        modifier(RevealsAsTrackRow(trackId: trackId))
    }
}

private struct RevealsAsTrackRow: ViewModifier {
    @Environment(UiStore.self)
    private var uiStore
    @Environment(\.revealedTrackRowInView)
    private var reportInView
    let trackId: String

    func body(content: Content) -> some View {
        let seq = uiStore.pendingAlbumReveal.flatMap { reveal in
            reveal.trackId == trackId ? reveal.seq : nil
        }
        content.onGeometryChange(for: RowVisibility?.self) { geometry in
            // Only a row in a scroll view is scrolled to.
            guard let seq, let visible = geometry.bounds(of: .scrollView)
            else { return nil }
            return RowVisibility(
                seq: seq,
                inView: visible.minY < 0.5
                    && visible.maxY > geometry.size.height - 0.5
            )
        } action: { visibility in
            if let visibility {
                reportInView(visibility.seq, visibility.inView)
            }
        }
        .anchorPreference(
            key: RevealedTrackRowKey.self,
            value: .bounds
        ) { bounds in
            seq.map {
                RevealedTrackRow(trackId: trackId, seq: $0, bounds: bounds)
            }
        }
    }
}

/// Whether the row reveal `seq` names sits in full view.
private struct RowVisibility: Equatable {
    let seq: Int
    let inView: Bool
}
