import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components (`mac.component.<thing>.<state>`), each state a
    /// row of the component's `preview.html`, drawn at the preview's width.
    /// A component with a capture file, `Components/<Component>.swift`,
    /// keeps its scenes there.
    static let componentScenes: [ShotScene] =
        [
            libraryRowScenes,
            partsComponentScenes,
            playbackComponentScenes,
            libraryComponentScenes,
            storageComponentScenes,
            importComponentScenes,
            basicsComponentScenes,
            browseComponentScenes,
            windowComponentScenes,
            controlsComponentScenes,
            settingsPaneComponentScenes,
            sidePaneComponentScenes,
            macAccentPickerScenes,
            macCastRowScenes,
            macCheckboxScenes,
            macCloseButtonScenes,
            macCopyTextScenes,
            macDisclosureScenes,
            macErrorDetailsScenes,
            macFieldRowScenes,
            macFileNameFieldScenes,
            macImportDestinationScenes,
            macInfoTipScenes,
            macStorageCellScenes,
            macStorageFooterScenes,
            macStorageToolbarScenes,
            macStorageWindowScenes,
            macTableScenes,
            macTokenScenes,
        ]
        .flatMap { $0 }

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

    /// A component's state: `view` in a column `width` points wide, over
    /// the elevated surface the preview draws every component on. A state's
    /// board is its preview row's cell, which holds the component at its top
    /// left with no margin. A `page` state is a preview with no rows, whose
    /// board is the whole page: the page sets the component in by its edge
    /// space, so the frame does too. `height` is room for the state; the
    /// picture's comparison ends at the board's height.
    static func component<Content: View>(
        _ id: String,
        page: Bool = false,
        width: CGFloat = 380,
        height: CGFloat,
        images: ImageStore? = nil,
        isReady: @escaping @MainActor () -> Bool = { true },
        @ViewBuilder view: @escaping @MainActor () -> Content
    ) -> ShotScene {
        let margin = page ? ThemeSpace.edge : 0
        let size = CGSize(
            width: width + 2 * margin,
            height: height + 2 * margin
        )
        return ShotScene(id: id, size: size) {
            let content = view()
                .padding(margin)
                .frame(
                    maxWidth: .infinity,
                    maxHeight: .infinity,
                    alignment: .topLeading
                )
                .background(Theme.surfaceElevated)
            // Every view that draws art reads the store from the
            // environment, as the app's window passes it.
            guard let images else {
                return ShotStage(
                    content.environment(ImageStore.stub()),
                    isReady: isReady
                )
            }
            return ShotStage(
                content.environment(images),
                images: images,
                isReady: isReady
            )
        }
    }
}

/// A `MacBox`: the box of rows the settings panes draw, as the design's
/// preview shows a row among others. A box outside a form is a `MacBox`,
/// the form style's own rows and surface without its page margin.
struct ShotBox<Content: View>: View {
    @ViewBuilder
    let content: Content

    var body: some View {
        MacBox { content }
    }
}

/// A settings form drawn in a component's frame: the form style's sections
/// without the page margin a pane puts round them.
struct ShotForm<Content: View>: View {
    @ViewBuilder
    let content: Content

    var body: some View {
        Form { content }
            .formStyle(.settingsWithoutMargin)
            .scrollDisabled(true)
    }
}
