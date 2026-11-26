import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macIdentifierBandScenes: [ShotScene] = [
        identifierBandScene(
            "mac.component.identifier-band.found",
            PreviewData.identifyRunBandBoard
        ),
        identifierBandScene(
            "mac.component.identifier-band.nothing-found",
            noMatchRun
        ),
        identifierBandScene(
            "mac.component.identifier-band.wrapped",
            PreviewData.identifyRunBandBoard,
            width: 300,
            height: 130,
            wrapsAtWidth: true
        ),
        identifierBandScene(
            "mac.component.identifier-band.left-out",
            leftOutRun
        ),
        identifierBandScene(
            "mac.component.identifier-band.title-search",
            titleSearchRun
        ),
        identifierBandScene(
            "mac.component.identifier-band.catalog-numbers",
            catalogNumbersRun
        ),
        identifierBandScene(
            "mac.component.identifier-band.catalog-numbers-shown",
            catalogNumbersRun,
            listsCatalogCandidates: true
        ),
        identifierBandScene(
            "mac.component.identifier-band.isrcs",
            isrcsRun
        ),
        isrcsShownScene(
            "mac.component.identifier-band.isrcs-shown"
        ),
    ]

    /// The run whose Disc ID and barcode matched nothing, so it searches by
    /// the folder's artist and title, which found a release.
    private static var titleSearchRun: BridgeIdentifyRun {
        var run = noMatchRun
        run.search = .searched(
            album: "Low Tide Hours",
            artist: "Mara Quill",
            chip: .init(look: .found, trailing: .nothing)
        )
        return run
    }

    /// A catalog number whose release both catalogs name, with the Disc ID
    /// and the barcode left out of the run.
    private static let leftOutRun = BridgeIdentifyRun(
        providers: [.musicBrainz, .discogs],
        discId: .read(discs: [
            BridgeDiscIdRow(
                discId: "v8n2Kq1Z…",
                lookup: .notAsked(reason: .leftOut),
                chip: .init(look: .off, trailing: .nothing)
            )
        ]),
        barcode: .rows(rows: [
            BridgeSignalValueRow(
                value: sampleBarcode,
                excluded: true,
                chip: .init(look: .off, trailing: .nothing)
            )
        ]),
        catalog: .numbers(
            rows: [
                BridgeSignalValueRow(
                    value: "HR-1042",
                    excluded: false,
                    chip: .init(look: .found, trailing: .nothing)
                )
            ],
            candidates: []
        ),
        isrc: .absent,
        search: .notNeeded,
        foundKinds: [.catalog],
        readingArtwork: false
    )

    /// The eleven ISRCs the files' tags carry, as MusicBrainz answered them.
    static let boardIsrcs = (1...11)
        .map {
            String(format: "USHR192001%02d", $0)
        }

    /// The board's found run with the files' ISRCs, which MusicBrainz found
    /// two releases for.
    private static var isrcsRun: BridgeIdentifyRun {
        var run = PreviewData.identifyRunBandBoard
        run.isrc = .read(
            isrcs: boardIsrcs,
            lookup: .found(count: 2, groups: []),
            chip: .init(look: .found, trailing: .nothing)
        )
        return run
    }

    /// The ISRC chip's +N pressed: the popover under the button lists every
    /// code, drawn in place as the app draws it, 10 points under the chip.
    private static func isrcsShownScene(_ id: String) -> ShotScene {
        component(id, width: 842, height: 259) {
            ZStack(alignment: .topLeading) {
                MacIdentifierBand(
                    run: isrcsRun,
                    onToggleLookup: { _ in },
                    onToggleCatalogAgreement: { _ in },
                    onRetryFailed: {},
                    onEditTitleSearch: { _, _ in },
                    listsCatalogCandidates: .constant(false)
                )
                moreValuesPopover(
                    boardIsrcs,
                    buttonCenter: isrcsMoreButtonCenter,
                    buttonBottom: 15.5
                )
            }
            .frame(width: 842, height: 259, alignment: .topLeading)
        }
    }

    /// Where the +N button's centre is from the left of the band, in the
    /// board's found run with ISRCs.
    private static let isrcsMoreButtonCenter: CGFloat = 446.6

    /// A chip's popover of every value, drawn in place as the board draws
    /// it: its right edge 75 points right of the +N button's centre, which is
    /// `buttonCenter` points from the left, its body 10 points under the
    /// button's bottom edge `buttonBottom`, its arrow on the button.
    static func moreValuesPopover(
        _ lines: [String],
        buttonCenter: CGFloat,
        buttonBottom: CGFloat
    ) -> some View {
        let width: CGFloat = 150 + 2 * 14
        return MacPopoverInPlace(arrowSide: .top, arrowOffset: width - 75) {
            MacIdentifierValuesPopover(lines: lines)
        }
        .padding(.leading, buttonCenter + 75 - width - popoverShadowRoom)
        .padding(
            .top,
            buttonBottom + 10 - popoverArrowLength - popoverShadowRoom
        )
    }

    /// No Disc ID, barcode or ISRC, and two catalog numbers that no
    /// release confirms, behind their count.
    private static let catalogNumbersRun = BridgeIdentifyRun(
        providers: [.musicBrainz, .discogs],
        discId: .absent,
        barcode: .absent,
        catalog: .numbers(
            rows: [],
            candidates: [
                BridgeCatalogCandidate(value: "HR-1042"),
                BridgeCatalogCandidate(value: "HR-1042-2"),
            ]
        ),
        isrc: .absent,
        search: .notNeeded,
        foundKinds: [],
        readingArtwork: false
    )

    /// The run whose Disc ID and barcode both catalogs answered with nothing.
    private static let noMatchRun = BridgeIdentifyRun(
        providers: [.musicBrainz, .discogs],
        discId: .read(discs: [
            BridgeDiscIdRow(
                discId: "v8n2Kq1Z…",
                lookup: .noMatch,
                chip: .init(look: .on, trailing: .nothing)
            )
        ]),
        barcode: .rows(rows: [
            BridgeSignalValueRow(
                value: sampleBarcode,
                excluded: false,
                chip: .init(look: .on, trailing: .nothing)
            )
        ]),
        catalog: .numbers(rows: [], candidates: []),
        isrc: .absent,
        search: .notNeeded,
        foundKinds: [],
        readingArtwork: false
    )

    private static func identifierBandScene(
        _ id: String,
        _ run: BridgeIdentifyRun,
        width: CGFloat = 580,
        height: CGFloat = 70,
        wrapsAtWidth: Bool = false,
        listsCatalogCandidates: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: width,
            height: height
        ) {
            MacIdentifierBand(
                run: run,
                onToggleLookup: { _ in },
                onToggleCatalogAgreement: { _ in },
                onRetryFailed: {},
                onEditTitleSearch: { _, _ in },
                listsCatalogCandidates: .constant(listsCatalogCandidates)
            )
            // The wrapped state's board holds the band in a narrow box, and
            // every other board lets the band take the width it needs.
            .frame(maxWidth: wrapsAtWidth ? width : nil, alignment: .leading)
        }
    }
}
