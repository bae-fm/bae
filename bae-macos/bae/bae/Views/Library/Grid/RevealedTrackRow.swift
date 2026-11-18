import SwiftUI

/// The row of the track a reveal names, as that row reports itself to the
/// grid that scrolls to it: the row exists only once the album's detail is
/// laid out, which the grid cannot see from its own slots.
struct RevealedTrackRow: Equatable {
    let trackId: String
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
        let revealed = uiStore.albumReveal?.trackId == trackId
        content.anchorPreference(
            key: RevealedTrackRowKey.self,
            value: .bounds
        ) { bounds in
            revealed
                ? RevealedTrackRow(trackId: trackId, bounds: bounds) : nil
        }
    }
}
