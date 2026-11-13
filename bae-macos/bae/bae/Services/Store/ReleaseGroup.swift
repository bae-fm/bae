import BaeKit
import Foundation

/// An album, as one or both sources describe it, with the pressings they
/// surfaced for it. Mirrors `BridgeReleaseGroup` — the grouping, the
/// cross-source merge and the pressing pairing all happen in bae-library; the UI
/// iterates the groups and their pressings and renders.
struct ReleaseGroup: Equatable, Identifiable {
    let id: String
    /// The album's title as each of its records states it, each distinct
    /// title once.
    let titles: [String]
    /// The album's artist as each of its records states it, each distinct
    /// credit once, worded.
    let artists: [String]
    /// Every label the album's pressings state, each once.
    let labels: [String]
    let coverArt: BridgeRemoteCover?
    /// Every source carrying this group, in the one order surfaces name
    /// sources in.
    let sources: [BridgeReleaseGroupSource]
    /// The card's rows, album by album: one section with no heading, or one
    /// per album where core split a card holding two albums of one catalog.
    let sections: [PressingSection]

    /// Every title the album has, in one line.
    var title: String {
        PressingText.alternatives(titles)
    }

    /// Every artist the album names, in one line; `nil` where none is named.
    var artist: String? {
        artists.isEmpty ? nil : PressingText.alternatives(artists)
    }

    /// Every row on the card, section by section.
    var pressings: [Pressing] {
        sections.flatMap(\.pressings)
    }

    var coverImageContent: ImageContent? {
        coverArt?.coverChoice.imageContent
    }

    init(bridge: BridgeReleaseGroup) {
        id = bridge.id
        titles = bridge.titles
        artists = bridge.artists.map(\.text)
        labels = bridge.labels
        coverArt = bridge.coverArt
        sources = bridge.sources
        sections = bridge.sections.map(PressingSection.init(bridge:))
    }
}

/// One album's rows on a card. Mirrors `BridgePressingSection`.
struct PressingSection: Equatable {
    /// The album the rows are pressings of, where core split the card's rows
    /// by album.
    let album: BridgeAlbumHeading?
    /// The rows offered: one per physical pressing, each carrying every
    /// source that lists it.
    let pressings: [Pressing]
    /// The rows of this album a run's agreement set aside, shown behind the
    /// list's "more" disclosure.
    let narrowedOut: [Pressing]

    init(bridge: BridgePressingSection) {
        album = bridge.album
        pressings = bridge.pressings.compactMap(Pressing.init(bridge:))
        narrowedOut = bridge.narrowedOut.compactMap(Pressing.init(bridge:))
    }
}
