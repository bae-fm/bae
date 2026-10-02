import SwiftUI

/// Where the parts of the album a pending reveal shows are laid out, as each
/// reports itself to the grid that scrolls to them: the album's card, its
/// detail once the detail shows what it loaded, and the row of the track the
/// reveal names. The detail and the row are drawn by the detail's own views,
/// which the grid cannot see from its slots.
struct RevealAnchors: Equatable {
    /// One part, for the reveal `seq` it answers, so a part laid out for an
    /// earlier reveal is never taken for a later one.
    struct Part: Equatable {
        let seq: Int
        let bounds: Anchor<CGRect>
    }

    /// The scroll view's content, whose place in the scroll view is where
    /// the scroll stands.
    var content: Anchor<CGRect>?
    var card: Part?
    var detail: Part?
    var row: Part?
}

struct RevealAnchorsKey: PreferenceKey {
    static let defaultValue = RevealAnchors()

    static func reduce(
        value: inout RevealAnchors,
        nextValue: () -> RevealAnchors
    ) {
        let next = nextValue()
        value.content = value.content ?? next.content
        value.card = value.card ?? next.card
        value.detail = value.detail ?? next.detail
        value.row = value.row ?? next.row
    }
}

extension View {
    /// Marks this view as `trackId`'s row, which a reveal naming the track
    /// scrolls into view and then flashes.
    func revealsAsTrackRow(_ trackId: String) -> some View {
        modifier(RevealsAsPart(kind: .row) { $0.trackId == trackId })
    }

    /// Marks this view as `albumId`'s detail showing what it loaded, which a
    /// reveal of the album shows whole under its card. The detail's height
    /// is its own only from here on: until then it is a stand-in.
    func revealsAsAlbumDetail(_ albumId: String) -> some View {
        modifier(RevealsAsPart(kind: .detail) { $0.albumId == albumId })
    }
}

/// Reports this view's bounds as the part of `kind` while the pending reveal
/// is one `answers` accepts.
private struct RevealsAsPart: ViewModifier {
    enum Kind {
        case detail
        case row
    }

    @Environment(UiStore.self)
    private var uiStore
    let kind: Kind
    let answers: (PendingAlbumReveal) -> Bool

    func body(content: Content) -> some View {
        let seq = uiStore.pendingAlbumReveal.flatMap {
            answers($0) ? $0.seq : nil
        }
        // A transform, not a value: the detail holds the rows, whose own
        // reports go up through it.
        content.transformAnchorPreference(
            key: RevealAnchorsKey.self,
            value: .bounds
        ) { anchors, bounds in
            guard let seq else { return }
            let part = RevealAnchors.Part(seq: seq, bounds: bounds)
            switch kind {
            case .detail:
                anchors.detail = part
            case .row:
                anchors.row = part
            }
        }
    }
}
