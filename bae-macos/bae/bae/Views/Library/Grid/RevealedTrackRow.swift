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
    let trackId: String

    func body(content: Content) -> some View {
        let seq = uiStore.pendingAlbumReveal.flatMap { reveal in
            reveal.trackId == trackId ? reveal.seq : nil
        }
        content.anchorPreference(
            key: RevealedTrackRowKey.self,
            value: .bounds
        ) { bounds in
            seq.map {
                RevealedTrackRow(trackId: trackId, seq: $0, bounds: bounds)
            }
        }
    }
}
