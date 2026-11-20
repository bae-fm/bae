import BaeKit
import SwiftUI

@testable import bae

// The import's search: the identifier band over Find online and its cards
// and pressings.

extension ShotScene {
    static let importSearchScenes: [ShotScene] =
        identifierChipScenes + identifierBandScenes + sourcedValueScenes
        + pressingRowScenes + releaseCardScenes + releaseGroupScenes
        + findOnlineScenes

    // MARK: Pressings

    /// The id a catalog gives its record of the pressing `releaseId`.
    private static func recordId(
        _ releaseId: String,
        _ catalog: BridgeLookupCatalog
    ) -> String {
        "\(releaseId)-\(catalog == .musicBrainz ? "mb" : "discogs")"
    }

    /// The record `source` holds of the pressing `releaseId`.
    private static func record(
        _ source: BridgeLookupCatalog,
        releaseId: String,
        year: Int32?,
        facts: BridgePressingFacts,
        barcode: String?
    ) -> BridgeMetadataResult {
        BridgeMetadataResult(
            source: source,
            releaseId: recordId(releaseId, source),
            year: year,
            facts: facts,
            barcodes: barcode.map { [$0] } ?? [],
            sourceGroupId: "component-low-tide-hours"
        )
    }

    /// What picking the pressing `releaseId` links the folder to.
    private static func pick(
        _ releaseId: String,
        _ source: BridgeLookupCatalog
    ) -> BridgePressingLink {
        BridgePressingLink(
            record: BridgeLookupRef(
                catalog: source,
                key: recordId(releaseId, source)
            ),
            partners: []
        )
    }

    /// A pressing of Low Tide Hours as the catalogs `sources` list it, whose
    /// records agree: its year, Harbor Records and the catalog number, in
    /// the format `facts` states.
    private static func pressing(
        releaseId: String,
        year: Int32?,
        catalog: String?,
        facts: BridgePressingFacts,
        sources: [BridgeLookupCatalog],
        barcode: String? = nil,
        chosen: Bool = false,
        inLibrary: Bool = false
    ) -> BridgePressing {
        BridgePressing(
            releases: sources.map {
                record(
                    $0,
                    releaseId: releaseId,
                    year: year,
                    facts: facts,
                    barcode: barcode
                )
            },
            sources: sources,
            labels: [
                BridgeLabelLine(
                    names: ["Harbor Records"],
                    catalogNumbers: catalog.map { [$0] } ?? []
                )
            ],
            pick: pick(releaseId, sources[0]),
            chosen: chosen,
            inLibrary: inLibrary
        )
    }

    /// The 1992 CD, which both catalogs list and the folder's Disc ID and
    /// barcode agree with.
    private static func cdPressing(
        chosen: Bool = false,
        inLibrary: Bool = false,
        barcode: String? = "0 41234 56789 2"
    ) -> BridgePressing {
        pressing(
            releaseId: "hr-1042",
            year: 1992,
            catalog: "HR-1042",
            facts: PreviewData.pressingFacts(
                country: "US",
                media: PreviewData.media(.cd)
            ),
            sources: [.musicBrainz, .discogs],
            barcode: barcode,
            chosen: chosen,
            inLibrary: inLibrary
        )
    }

    /// The areas MusicBrainz and Discogs state for the differing CD: the area
    /// alone, with no media, status or packaging.
    private static func differingAreas(
        cd: BridgePressingFacts,
        europe: BridgePressingFacts
    ) -> [BridgeTermsStatement] {
        func area(_ facts: BridgePressingFacts) -> [BridgeFactTerm] {
            bridgePressingSummary(
                facts: BridgePressingFacts(
                    area: facts.area,
                    media: [],
                    status: nil,
                    packaging: nil,
                    discogsDetails: []
                )
            )
        }
        return [
            BridgeTermsStatement(terms: area(cd), catalogs: [.musicBrainz]),
            BridgeTermsStatement(terms: area(europe), catalogs: [.discogs]),
        ]
    }

    /// The same CD where MusicBrainz and Discogs state its year, country and
    /// notes differently.
    private static func differingPressing() -> BridgePressing {
        let cd = PreviewData.pressingFacts(
            country: "US",
            media: PreviewData.media(.cd)
        )
        let europe = PreviewData.pressingFacts(
            region: .europe,
            media: PreviewData.media(.cd)
        )
        return BridgePressing(
            releases: [
                record(
                    .musicBrainz,
                    releaseId: "hr-1042",
                    year: 1992,
                    facts: cd,
                    barcode: "0 41234 56789 2"
                ),
                record(
                    .discogs,
                    releaseId: "hr-1042",
                    year: 1993,
                    facts: europe,
                    barcode: "0 41234 56789 2"
                ),
            ],
            sources: [.musicBrainz, .discogs],
            labels: [
                BridgeStatedLabelLine(
                    names: ["Harbor Records"],
                    catalogNumbers: [
                        BridgeTextStatement(text: "HR-1042", catalogs: [])
                    ]
                )
            ],
            years: [
                BridgeYearStatement(year: 1992, catalogs: [.musicBrainz]),
                BridgeYearStatement(year: 1993, catalogs: [.discogs]),
            ],
            barcodes: [
                BridgeTextStatement(text: "0 41234 56789 2", catalogs: [])
            ],
            documentFailure: nil,
            pick: pick("hr-1042", .musicBrainz),
            areas: differingAreas(cd: cd, europe: europe),
            media: [
                BridgeTermsStatement(
                    terms: bridgeMediaTerms(media: cd.media),
                    catalogs: []
                )
            ],
            details: [],
            chosen: false,
            pickBeingRead: false,
            inLibrary: false
        )
    }

    /// What the folder's own text agrees with about a pressing.
    private static func agreements(
        catalog: Bool = false,
        notes: [BridgeTextStatement] = []
    ) -> BridgeAgreements {
        BridgeAgreements(
            discId: true,
            barcode: true,
            catalog: catalog,
            label: false,
            year: false,
            country: false,
            notes: notes
        )
    }

    @ViewBuilder
    private static func pressingRow(
        _ bridge: BridgePressing,
        agreements: BridgeAgreements? = nil
    ) -> some View {
        if let pressing = Pressing(bridge: bridge) {
            MacPressingRow(
                pressing: pressing,
                isImporting: false,
                agreements: agreements,
                isSelected: bridge.chosen,
                onSelect: { _ in }
            )
        }
    }

    private static func pressingRowScene(
        _ id: String,
        _ bridge: BridgePressing,
        agreements: BridgeAgreements? = nil,
        height: CGFloat = 80
    ) -> ShotScene {
        component(
            id,
            width: 560,
            height: height
        ) {
            pressingRow(bridge, agreements: agreements)
                .frame(width: 560, alignment: .leading)
        }
    }

    private static let pressingRowScenes: [ShotScene] = [
        pressingRowScene("mac.component.pressing-row.plain", cdPressing()),
        pressingRowScene(
            "mac.component.pressing-row.badges",
            cdPressing(),
            agreements: agreements()
        ),
        pressingRowScene(
            "mac.component.pressing-row.selected",
            cdPressing(chosen: true),
            agreements: agreements()
        ),
        pressingRowScene(
            "mac.component.pressing-row.in-library",
            cdPressing(inLibrary: true)
        ),
        pressingRowScene(
            "mac.component.pressing-row.records-differ",
            differingPressing(),
            agreements: agreements(
                catalog: true,
                notes: [
                    BridgeTextStatement(
                        text: "first pressing",
                        catalogs: [.musicBrainz]
                    ),
                    BridgeTextStatement(
                        text: "club edition",
                        catalogs: [.discogs]
                    ),
                ]
            ),
            height: 100
        ),
        pressingRowScene(
            "mac.component.pressing-row.notes",
            cdPressing(),
            agreements: agreements(
                notes: [
                    BridgeTextStatement(text: "first pressing", catalogs: [])
                ]
            ),
            height: 90
        ),
        pressingRowScene(
            "mac.component.pressing-row.year-unknown",
            pressing(
                releaseId: "hrlp-1042",
                year: nil,
                catalog: nil,
                facts: PreviewData.pressingFacts(
                    media: PreviewData.media(.cd)
                ),
                sources: [.discogs]
            ),
            height: 70
        ),
    ]

    // MARK: Sourced value

    private static func sourcedValueScene(
        _ id: String,
        _ values: [StatedValue],
        strong: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: 284,
            height: 40
        ) {
            if strong {
                MacSourcedValue(values)
                    .themeText(.strong)
                    .monospacedDigit()
            }
            else {
                MacSourcedValue(values)
            }
        }
    }

    private static let sourcedValueScenes: [ShotScene] = [
        sourcedValueScene(
            "mac.component.sourced-value.one-record",
            [StatedValue(value: "1992", catalogs: [])]
        ),
        sourcedValueScene(
            "mac.component.sourced-value.records-differ",
            [
                StatedValue(value: "1992", catalogs: [.musicBrainz]),
                StatedValue(value: "1993", catalogs: [.discogs]),
            ],
            strong: true
        ),
    ]

    // MARK: Identifier chips

    private static func capsule(
        _ source: BridgeLookupCatalog,
        _ lookup: BridgeLookupState
    ) -> some View {
        ProviderCapsule(source: source, lookup: lookup, onRetry: {})
    }

    private static func identifierChipScene<Chip: View>(
        _ id: String,
        _ chip: @escaping @MainActor () -> Chip
    ) -> ShotScene {
        component(
            id,
            width: 344,
            height: 40
        ) {
            chip()
        }
    }

    private static let sampleBarcode = "0 41234 56789 2"

    private static let identifierChipScenes: [ShotScene] = [
        identifierChipScene("mac.component.identifier-chip.filled") {
            MacIdentifierChip(label: "Barcode", value: sampleBarcode) {
                EmptyView()
            }
        },
        identifierChipScene("mac.component.identifier-chip.outlined") {
            MacIdentifierChip(
                label: "Barcode",
                value: sampleBarcode,
                style: .outlined
            ) { EmptyView() }
        },
        identifierChipScene("mac.component.identifier-chip.struck") {
            MacIdentifierChip(
                label: "Barcode",
                value: sampleBarcode,
                style: .struck
            ) { EmptyView() }
        },
        identifierChipScene("mac.component.identifier-chip.dash") {
            MacIdentifierChip(label: "Barcode")
        },
        identifierChipScene("mac.component.identifier-chip.found") {
            MacIdentifierChip(label: "Barcode", value: sampleBarcode) {
                capsule(.musicBrainz, .found(count: 2, groups: []))
                capsule(.discogs, .noMatch)
            }
        },
        identifierChipScene("mac.component.identifier-chip.searching") {
            MacIdentifierChip(label: "Barcode", value: sampleBarcode) {
                capsule(.musicBrainz, .found(count: 2, groups: []))
                capsule(.discogs, .lookingUp)
            }
        },
    ]

    // MARK: Identifier band

    /// The run where Disc ID and barcode each wait on, or found nothing
    /// from, the catalog that answers them.
    private static func bandRun(
        _ lookup: BridgeLookupState
    ) -> BridgeIdentifyRun {
        BridgeIdentifyRun(
            providers: [.musicBrainz, .discogs],
            discId: .read(discs: [
                BridgeDiscIdRow(discId: "v8n2Kq1Z…", lookup: lookup)
            ]),
            barcode: .rows(rows: [
                BridgeSignalValueRow(
                    value: sampleBarcode,
                    excluded: false,
                    cells: [
                        BridgeProviderCell(source: .discogs, lookup: lookup)
                    ]
                )
            ]),
            catalog: .numbers(rows: [], candidates: []),
            isrc: .absent,
            search: .notNeeded,
            readingArtwork: false
        )
    }

    private static func identifierBandScene(
        _ id: String,
        _ run: BridgeIdentifyRun,
        width: CGFloat = 580,
        height: CGFloat = 70
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
                onEditTitleSearch: { _, _ in }
            )
        }
    }

    private static let identifierBandScenes: [ShotScene] = [
        identifierBandScene(
            "mac.component.identifier-band.found",
            PreviewData.identifyRunBandBoard
        ),
        identifierBandScene(
            "mac.component.identifier-band.searching",
            bandRun(.lookingUp)
        ),
        identifierBandScene(
            "mac.component.identifier-band.nothing-found",
            bandRun(.noMatch)
        ),
        identifierBandScene(
            "mac.component.identifier-band.wrapped",
            PreviewData.identifyRunBandBoard,
            width: 300,
            height: 130
        ),
    ]

    // MARK: Release card and group

    /// The album's cover, which the catalog serves under the album's title.
    private static let lowTideHoursCover: BridgeRemoteCover = {
        let image = BridgeRemoteImageSet(url: "Low Tide Hours", downscaled: [])
        return BridgeRemoteCover(
            coverChoice: BridgeCoverChoice(
                selection: .remoteCover(
                    selection: BridgeRemoteCoverSelection(
                        image: image,
                        source: .musicBrainz
                    )
                ),
                image: .remote(image: image)
            ),
            sourceName: "Cover Art Archive",
            label: .lead(releaseGroup: true)
        )
    }()

    /// Low Tide Hours by Mara Quill on Harbor Records, as the catalogs
    /// `sources` list it, with `pressings` under it.
    private static func lowTideHoursGroup(
        titles: [String] = ["Low Tide Hours"],
        sources: [BridgeCatalog] = [.musicBrainz, .discogs],
        pressings: [BridgePressing] = []
    ) -> ReleaseGroup {
        ReleaseGroup(
            bridge: BridgeReleaseGroup(
                id: "component-low-tide-hours",
                titles: titles,
                artists: [.named("Mara Quill")],
                labels: ["Harbor Records"],
                coverArt: lowTideHoursCover,
                sources: sources.map {
                    BridgeReleaseGroupSource(
                        source: $0,
                        groupUrl: $0 == .musicBrainz
                            ? "https://musicbrainz.org/release-group/component-low-tide-hours"
                            : "https://www.discogs.com/master/component-low-tide-hours",
                        albumLinksUnread: false
                    )
                },
                sections: [
                    BridgePressingSection(
                        album: nil,
                        pressings: pressings,
                        narrowedOut: []
                    )
                ]
            )
        )
    }

    /// The preview's wide column.
    private static let wideColumn: CGFloat = 842

    private static func releaseCardScene(
        _ id: String,
        _ group: ReleaseGroup
    ) -> ShotScene {
        component(
            id,
            width: wideColumn,
            height: 70,
            images: coverGalleryImages()
        ) {
            MacReleaseCard(group: group)
                .frame(width: wideColumn, alignment: .leading)
        }
    }

    /// `MacReleaseCard`: both catalogs, none yet, and two titles in one line.
    private static let releaseCardScenes: [ShotScene] = [
        releaseCardScene(
            "mac.component.release-card.two-catalogs",
            lowTideHoursGroup()
        ),
        releaseCardScene(
            "mac.component.release-card.no-catalog-yet",
            lowTideHoursGroup(sources: [])
        ),
        releaseCardScene(
            "mac.component.release-card.long-title",
            lowTideHoursGroup(
                titles: ["Low Tide Hours", "Low Tide Hours (Remastered)"],
                sources: [.musicBrainz]
            )
        ),
    ]

    /// The 1992 CD as Europe's pressing, which the folder's catalog number
    /// agrees with.
    private static func europeanCdPressing() -> BridgePressing {
        pressing(
            releaseId: "hr-1042-eu",
            year: 1992,
            catalog: "HR-1042",
            facts: PreviewData.pressingFacts(
                region: .europe,
                media: PreviewData.media(.cd)
            ),
            sources: [.musicBrainz, .discogs]
        )
    }

    /// The 2014 LP reissue, which the library holds.
    private static func lpReissuePressing() -> BridgePressing {
        pressing(
            releaseId: "hrlp-1042",
            year: 2014,
            catalog: "HRLP-1042",
            facts: PreviewData.pressingFacts(
                media: PreviewData.media(.vinyl),
                discogsDetails: [.reissue]
            ),
            sources: [.musicBrainz, .discogs],
            inLibrary: true
        )
    }

    private static func releaseGroupScene(
        _ id: String,
        _ group: ReleaseGroup,
        agreements: [String: BridgeAgreements],
        height: CGFloat
    ) -> ShotScene {
        component(
            id,
            width: wideColumn,
            height: height,
            images: coverGalleryImages()
        ) {
            MacReleaseGroup(
                group: group,
                isImporting: false,
                agreements: agreements,
                onSelect: { _ in }
            )
            .frame(width: wideColumn, alignment: .leading)
        }
    }

    /// `MacReleaseGroup`: the album over its one pressing, picked, and over
    /// three, the last in the library.
    private static let releaseGroupScenes: [ShotScene] = [
        releaseGroupScene(
            "mac.component.release-group.one-pressing",
            lowTideHoursGroup(pressings: [
                cdPressing(chosen: true, barcode: nil)
            ]),
            agreements: [recordId("hr-1042", .musicBrainz): agreements()],
            height: 150
        ),
        releaseGroupScene(
            "mac.component.release-group.several-pressings",
            lowTideHoursGroup(pressings: [
                cdPressing(barcode: nil), europeanCdPressing(),
                lpReissuePressing(),
            ]),
            agreements: [
                recordId("hr-1042", .musicBrainz): agreements(),
                recordId("hr-1042-eu", .musicBrainz): BridgeAgreements(
                    discId: false,
                    barcode: false,
                    catalog: true,
                    label: false,
                    year: false,
                    country: false,
                    notes: []
                ),
            ],
            height: 270
        ),
    ]

    // MARK: Find online

    private static func findOnline() -> some View {
        importStores(
            MacFindOnline.preview(
                state: PreviewData.searchStateNotFound,
                showsDiscogsBar: true
            )
            .frame(width: 580, height: 330, alignment: .top)
        )
    }

    private static let findOnlineScenes: [ShotScene] = [
        component(
            "mac.component.find-online.default",
            page: true,
            width: 580,
            height: 360,
            images: PreviewData.artImageStore()
        ) {
            findOnline()
        }
    ]
}
