import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacLibraryRow`'s states: a search album and track, an artist at rest,
    /// hovered, selected and loading, on a page, and a credit.
    static let macLibraryRowScenes: [ShotScene] = [
        libraryRow(
            "mac.component.library-row.search-album",
            title: "Low Tide Hours",
            subtitle: "Mara Quill (1992)"
        ),
        libraryRow(
            "mac.component.library-row.search-track",
            title: "Harbor Lights",
            subtitle: "Mara Quill - Low Tide Hours",
            trailing: "4:32",
            art: "Low Tide Hours"
        ),
        libraryRow(
            "mac.component.library-row.hover",
            title: "Mara Quill",
            subtitle: "3 albums",
            hover: true
        ),
        libraryRow(
            "mac.component.library-row.artist",
            title: "Mara Quill",
            subtitle: "12 albums"
        ),
        libraryRow(
            "mac.component.library-row.selected",
            title: "Odile Fen",
            subtitle: "3 albums",
            selected: true
        ),
        libraryRow(
            "mac.component.library-row.loading",
            loading: true
        ),
        libraryRow(
            "mac.component.library-row.on-a-page-work",
            title: "Low Tide Suite",
            subtitle: "Ada Lindqvist",
            art: "Blue Room Sessions",
            bleed: true
        ),
        libraryRow(
            "mac.component.library-row.on-a-page-part",
            title: "II. Slack Water",
            subtitle: "Ada Lindqvist",
            art: "Blue Room Sessions",
            bleed: true,
            part: true
        ),
        libraryRow(
            "mac.component.library-row.on-a-page-hover",
            title: "Harbor Waltz",
            subtitle: "Ada Lindqvist",
            art: "Glass Harbor",
            hover: true,
            bleed: true
        ),
        libraryRow(
            "mac.component.library-row.credit",
            title: "Paper Lanterns",
            subtitle: "Arranger",
            credit: true
        ),
    ]

    /// One state of `MacLibraryRow`, in the preview's 380pt column.
    private static func libraryRow(
        _ id: String,
        title: String? = nil,
        subtitle: String? = nil,
        trailing: String? = nil,
        art: String? = nil,
        hover: Bool = false,
        selected: Bool = false,
        loading: Bool = false,
        bleed: Bool = false,
        credit: Bool = false,
        part: Bool = false
    ) -> ShotScene {
        component(id, height: 76, images: coverStore()) {
            MacLibraryRow(
                picture: art.map {
                    BridgeImageRef(id: $0, version: "1", imageType: .cover)
                },
                title: title,
                subtitle: subtitle,
                trailing: trailing,
                hover: hover,
                selected: selected,
                loading: loading,
                bleed: bleed,
                credit: credit,
                part: part
            )
            .frame(width: 380, alignment: .leading)
        }
    }
}
