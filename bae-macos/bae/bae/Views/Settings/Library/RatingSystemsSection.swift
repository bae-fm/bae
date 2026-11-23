import BaeKit
import SwiftUI

/// The Ratings section of Settings' Library tab: how the library rates its
/// albums and its tracks, each Stars, Heart or None, for everyone in the
/// library. An admin changes each in a pop-up; a member sees the system as a
/// plain value. The tab follows the systems while it shows, and the section
/// draws nothing until the first value arrives.
struct RatingSystemsSection: View {
    @Environment(RatingSystemsStore.self)
    private var store

    var body: some View {
        if store.systems != nil {
            MacSection {
                RatingSystemRow(
                    title: "Albums",
                    items: .albums
                )
                RatingSystemRow(
                    title: "Tracks",
                    items: .tracks
                )
            } header: {
                Text("Ratings")
            } footer: {
                Text("Applies to everyone in this library")
            }
        }
    }
}

/// One row of the Ratings section: the rating system for albums or for
/// tracks, as a pop-up for someone who can change it and as a plain value for
/// someone who cannot. The pop-up shows the person's pick until its write
/// returns.
private struct RatingSystemRow: View {
    let title: LocalizedStringKey
    let items: BridgeRatedItems

    @Environment(RatingSystemsStore.self)
    private var store

    var body: some View {
        if let systems = store.systems {
            row(
                current: items == .albums ? systems.albums : systems.tracks,
                canChange: systems.canChange
            )
        }
    }

    @ViewBuilder
    private func row(current: BridgeRatingSystem, canChange: Bool) -> some View
    {
        if canChange {
            MacRow(title) {
                MacPopUpButton(
                    label: Text(title),
                    selection: Binding(
                        get: { store.pending[items] ?? current },
                        set: { store.choose(items, $0) }
                    )
                ) {
                    for system in BridgeRatingSystem.allCases {
                        MacPopUpOption(verbatim: system.title, value: system)
                    }
                }
                .fixedSize()
            }
        }
        else {
            MacRow(title, value: current.title)
        }
    }
}
