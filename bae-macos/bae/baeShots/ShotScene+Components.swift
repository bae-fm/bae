import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components (`mac.component.<thing>.<state>`), each state a
    /// row of the component's `preview.html`, drawn at the preview's width.
    static let componentScenes: [ShotScene] =
        libraryRowScenes + settingsComponentScenes + partsComponentScenes

    /// `MacLibraryRow`'s states.
    private static let libraryRowScenes: [ShotScene] = [
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

    /// One state of `MacLibraryRow`, in the preview's 380pt column with the
    /// 8pt a page row's fill reaches past it.
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
        ShotScene(
            id: id,
            size: CGSize(width: 396, height: 76)
        ) {
            let images = ImageStore(
                fetchLibraryImageBytes: { PreviewData.previewArtData($0.id) }
            )
            return ShotStage(
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
                .frame(width: 380)
                .frame(
                    maxWidth: .infinity,
                    maxHeight: .infinity,
                    alignment: .top
                )
                .environment(images),
                images: images
            )
        }
    }

    /// A component's state: `view` at the top left of a frame `width` points
    /// wide, which is the preview's column with the 8pt it leaves round a
    /// row.
    static func component<Content: View>(
        _ id: String,
        width: CGFloat = 396,
        height: CGFloat,
        @ViewBuilder view: @escaping @MainActor () -> Content
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: width, height: height)) {
            ShotStage(
                view()
                    .padding(8)
                    .frame(
                        maxWidth: .infinity,
                        maxHeight: .infinity,
                        alignment: .topLeading
                    )
            )
        }
    }
}

/// A `MacBox`: the box a grouped form draws round its rows, as the design's
/// preview shows a row among others.
struct ShotBox<Content: View>: View {
    @ViewBuilder
    let content: Content

    var body: some View {
        Form {
            Section { content }
        }
        .formStyle(.grouped)
        .scrollDisabled(true)
    }
}
