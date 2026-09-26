#if DEBUG
    import BaeKit
    import Foundation

    extension PreviewData {
        // MARK: - Album Details

        private static func makeTracks(
            _ names: [String],
            artist: String,
            side: Int32 = 1,
            positionText: (Int) -> String = {
                String($0 + 1)
            },
        ) -> [BridgeTrack] {
            names.enumerated()
                .map { index, name in
                    let durationMs = Int64((170 + (index * 37) % 170) * 1000)
                    return BridgeTrack(
                        id: "t-d\(side)-\(index + 1)",
                        title: name,
                        side: side,
                        trackNumber: Int32(index + 1),
                        durationMs: durationMs,
                        durationClock: bridgeClock(ms: durationMs),
                        artistNames: artist,
                        displayArtist: nil,
                        positionText: positionText(index),
                    )
                }
        }

        /// The group's runtime in whole minutes, rounded half up as core does.
        private static func groupDuration(_ tracks: [BridgeTrack])
            -> BridgeDurationUnits?
        {
            let ms = tracks.reduce(Int64(0)) { $0 + ($1.durationMs ?? 0) }
            guard ms > 0 else { return nil }
            let minutes = (ms + 30_000) / 60_000
            return minutes >= 60
                ? .hoursAndMinutes(
                    hours: UInt64(minutes / 60),
                    minutes: UInt64(minutes % 60)
                )
                : .minutesOnly(minutes: UInt64(minutes))
        }

        /// A two-part release's tracks plus the matching per-side groups, so the
        /// preview renders real "Side A" / "Disc 2" headers.
        private static func twoSide(
            artist: String,
            isVinyl: Bool,
            disc1: [String],
            disc2: [String],
        ) -> (tracks: [BridgeTrack], groups: [BridgeTrackGroup]) {
            func side(
                _ sideNumber: Int32,
                letter: String,
                names: [String],
            ) -> (tracks: [BridgeTrack], group: BridgeTrackGroup) {
                let tracks = makeTracks(names, artist: artist, side: sideNumber)
                {
                    index in
                    isVinyl ? "\(letter)\(index + 1)" : "\(index + 1)"
                }
                let groupSide: BridgeTrackSide =
                    isVinyl
                    ? .sided(sideLetter: letter) : .disc(disc: sideNumber)
                return (
                    tracks,
                    BridgeTrackGroup(
                        side: groupSide,
                        headerKey: isVinyl
                            ? "core.track.side" : "core.track.disc",
                        tracks: tracks,
                        totalDuration: groupDuration(tracks)
                    )
                )
            }
            let (tracks1, group1) = side(1, letter: "A", names: disc1)
            let (tracks2, group2) = side(2, letter: "B", names: disc2)
            return (tracks1 + tracks2, [group1, group2])
        }

        private struct AlbumFixture {
            let id: String
            let title: String
            let artist: String
            let year: Int32
        }

        private struct ReleaseIdentifiers {
            let label: String
            let catalogNumber: String
            let area: BridgeReleaseArea
        }

        private enum ReleaseTrackLayout {
            case flat([String])
            case twoPart(first: [String], second: [String])
        }

        private struct ReleaseFixture {
            let name: String?
            let year: Int32?
            let media: [BridgeMediaCount]
            let tracks: ReleaseTrackLayout
            let identifiers: ReleaseIdentifiers?

            init(
                media: [BridgeMediaCount],
                tracks: ReleaseTrackLayout,
                name: String? = nil,
                year: Int32? = nil,
                identifiers: ReleaseIdentifiers? = nil,
            ) {
                self.name = name
                self.year = year
                self.media = media
                self.tracks = tracks
                self.identifiers = identifiers
            }
        }

        /// The fixture's tracks, and the sides or discs they are grouped by.
        private static func makeTrackLayout(
            album: AlbumFixture,
            fixture: ReleaseFixture,
        ) -> (tracks: [BridgeTrack], groups: [BridgeTrackGroup]) {
            switch fixture.tracks {
            case .flat(let names):
                let tracks = makeTracks(names, artist: album.artist)
                return (
                    tracks,
                    [
                        BridgeTrackGroup(
                            side: .flat,
                            headerKey: nil,
                            tracks: tracks,
                            totalDuration: groupDuration(tracks)
                        )
                    ]
                )
            case .twoPart(let first, let second):
                return twoSide(
                    artist: album.artist,
                    isVinyl: fixture.media.contains {
                        $0.medium == .vinyl || $0.medium == .cassette
                    },
                    disc1: first,
                    disc2: second,
                )
            }
        }

        private static func makeRelease(
            album: AlbumFixture,
            id: String,
            fixture: ReleaseFixture,
        ) -> BridgeRelease {
            let (tracks, groups) = makeTrackLayout(
                album: album,
                fixture: fixture
            )
            let year = fixture.year ?? album.year
            let facts = BridgePressingFacts(
                area: fixture.identifiers?.area,
                media: fixture.media,
                status: nil,
                packaging: nil,
                discogsDetails: []
            )
            return BridgeRelease(
                id: id,
                albumId: album.id,
                name: fixture.name.map { .named(name: $0) }
                    ?? .described(year: year, media: fixture.media),
                year: year,
                label: fixture.identifiers?.label,
                catalogNumber: fixture.identifiers?.catalogNumber,
                facts: facts,
                pressingSummary: bridgePressingSummary(facts: facts),
                pressingDetails: bridgePressingDetails(facts: facts),
                storageState: .local,
                pinned: false,
                storageActions: [],
                transferAction: nil,
                tracks: tracks,
                trackGroups: groups,
                files: [],
                sourceAudio: nil,
                imageFiles: [],
                coverFiles: [],
                galleryItems: [],
                // Every catalog on the first album's first pressing, so the
                // expansion preview shows what a release a MusicBrainz pick
                // linked all the way out reads like; the rest are the ordinary
                // case of a library nobody has identified.
                records: album.id == "a-01" ? releaseRecordsEveryCatalog : [],
                totalDuration: groupDuration(tracks),
                fileCount: 0,
                totalSize: 0,
                cover: nil,
            )
        }

        private static func makeDetail(
            album: AlbumFixture,
            primary: ReleaseFixture,
            additional: [ReleaseFixture] = [],
        ) -> BridgeAlbumDetail {
            let fixtures = [primary] + additional
            let releaseIds = fixtures.indices.map { index in
                fixtures.count == 1
                    ? "rel-\(album.id)" : "rel-\(album.id)-\(index)"
            }
            let releases = zip(releaseIds, fixtures)
                .map { id, fixture in
                    makeRelease(album: album, id: id, fixture: fixture)
                }
            return BridgeAlbumDetail(
                album: BridgeAlbum(
                    id: album.id,
                    title: album.title,
                    year: album.year,
                    isCompilation: false,
                    artistNames: album.artist,
                    releaseIds: releaseIds,
                    primaryReleaseId: releaseIds[0],
                    cover: nil,
                ),
                releases: releases,
            )
        }

        private static func makeDetail(
            album: AlbumFixture,
            tracks: [String],
            media: [BridgeMediaCount],
        ) -> BridgeAlbumDetail {
            makeDetail(
                album: album,
                primary: ReleaseFixture(
                    media: media,
                    tracks: .flat(tracks),
                ),
            )
        }

        private static func makeDetailTwoPart(
            album: AlbumFixture,
            first: [String],
            second: [String],
            media: [BridgeMediaCount],
        ) -> BridgeAlbumDetail {
            makeDetail(
                album: album,
                primary: ReleaseFixture(
                    media: media,
                    tracks: .twoPart(first: first, second: second),
                    identifiers: ReleaseIdentifiers(
                        label: "Some Label",
                        catalogNumber: "CAT-001",
                        area: .country(code: "US"),
                    ),
                ),
            )
        }

        /// The ordered album payloads (a-01..a-23). The single source of truth:
        /// `albums` is `albumDetailList.map(\.album)` and `albumDetails` is keyed by
        /// `album.id`, so the grid summaries and the seeded `releaseDetails` always
        /// agree on release ids.
        static let albumDetailList: [BridgeAlbumDetail] = [
            makeDetail(
                album: .init(
                    id: "a-01",
                    title: "Neon Frequencies",
                    artist: "The Midnight Signal",
                    year: 2023,
                ),
                tracks: [
                    "Broadcast", "Static Dreams", "Frequency Drift",
                    "Night Transmission",
                    "Signal Lost", "Airwave", "Carrier Wave", "Sign Off",
                ],
                media: [BridgeMediaCount(medium: .digital, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-02",
                    title: "Pacific Standard",
                    artist: "Glass Harbor",
                    year: 2019,
                ),
                tracks: [
                    "Coastal", "Tide Pool", "Harbor Lights", "Salt Air",
                    "Driftwood", "Fog Horn",
                    "Pier 17", "Last Ferry",
                ],
                media: [BridgeMediaCount(medium: .vinyl, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-03",
                    title: "Proof by Induction",
                    artist: "Velvet Mathematics",
                    year: 2021,
                ),
                tracks: [
                    "Axiom", "Recursive", "Limit Theorem", "Derivative",
                    "Integral", "Convergence",
                    "QED",
                ],
                media: [BridgeMediaCount(medium: .cd, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-04",
                    title: "Seconds",
                    artist: "The Borrowed Time",
                    year: 1974,
                ),
                primary: .init(
                    media: [BridgeMediaCount(medium: .vinyl, count: 1)],
                    tracks: .flat(
                        [
                            "Tick", "Borrowed", "Overdue", "Extension",
                            "Final Notice",
                            "Grace Period",
                        ]
                    ),

                    year: 1974,
                ),
                additional: [
                    .init(
                        media: [BridgeMediaCount(medium: .cd, count: 2)],
                        tracks: .twoPart(
                            first: [
                                "Tick", "Borrowed", "Overdue", "Extension",
                                "Final Notice",
                                "Grace Period",
                            ],
                            second: [
                                "Overtime", "Second Chance", "Borrowed (Demo)",
                                "Final Notice (Live)",
                            ],
                        ),
                        name: "1996 Reissue",
                        year: 1996,
                    )
                ]
            ),
            makeDetail(
                album: .init(
                    id: "a-05",
                    title: "Window Sill",
                    artist: "Apartment Garden",
                    year: 2020,
                ),
                tracks: [
                    "Basil", "Morning Light", "Terracotta", "Propagation",
                    "Root Bound",
                    "Water Day", "New Growth",
                ],
                media: [BridgeMediaCount(medium: .digital, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-06",
                    title: "Fuel Weight",
                    artist: "The Cold Equations",
                    year: 2018,
                ),
                tracks: [
                    "Launch Window", "Trajectory", "Orbital Decay", "Reentry",
                    "Terminal Velocity",
                    "Escape", "Gravity Well",
                ],
                media: [BridgeMediaCount(medium: .vinyl, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-07",
                    title: "Tomorrow's Forecast",
                    artist: "Newspaper Weather",
                    year: 2023,
                ),
                tracks: [
                    "Partly Cloudy", "High Pressure", "Cold Front",
                    "Scattered Showers",
                    "Clearing Skies", "Weekend Outlook",
                ],
                media: [BridgeMediaCount(medium: .digital, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-08",
                    title: "Alphabetical",
                    artist: "The Filing Cabinets",
                    year: 2017,
                ),
                tracks: [
                    "A-D", "E-H", "I-L", "M-P", "Q-T", "U-Z", "Miscellaneous",
                ],
                media: [BridgeMediaCount(medium: .cd, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-09",
                    title: "Level 4",
                    artist: "Parking Structure",
                    year: 2021,
                ),
                tracks: [
                    "Entrance", "Spiral Up", "Compact Only", "Reserved",
                    "Exit Ticket",
                    "Night Rate",
                ],
                media: [BridgeMediaCount(medium: .digital, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-10",
                    title: "Dial Tone",
                    artist: "The Last Payphone",
                    year: 2019,
                ),
                tracks: [
                    "Insert Coin", "Area Code", "Long Distance", "Collect Call",
                    "Busy Signal",
                    "Disconnected",
                ],
                media: [BridgeMediaCount(medium: .cassette, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-11",
                    title: "Set Theory",
                    artist: "Velvet Mathematics",
                    year: 2019,
                ),
                tracks: [
                    "Union", "Intersection", "Complement", "Subset",
                    "Empty Set", "Cardinality",
                ],
                media: [BridgeMediaCount(medium: .cd, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-12",
                    title: "Interest",
                    artist: "The Borrowed Time",
                    year: 2020,
                ),
                tracks: [
                    "Principal", "Compound", "Balloon Payment", "Amortization",
                    "Default",
                    "Refinance",
                ],
                media: [BridgeMediaCount(medium: .digital, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-13",
                    title: "Grow Light",
                    artist: "Apartment Garden",
                    year: 2022,
                ),
                tracks: [
                    "Spectrum", "Photosynthesis", "Chlorophyll", "Dormancy",
                    "Spring Bloom",
                    "Perennial",
                ],
                media: [BridgeMediaCount(medium: .vinyl, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-14",
                    title: "Landlocked",
                    artist: "Glass Harbor",
                    year: 2022,
                ),
                tracks: [
                    "Dry Dock", "Anchor", "Barnacles", "Rust", "Restoration",
                    "Launch Day",
                    "Open Water",
                ],
                media: [BridgeMediaCount(medium: .cd, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-15",
                    title: "Express",
                    artist: "The Checkout Lane",
                    year: 2023,
                ),
                tracks: [
                    "15 Items", "Price Check", "Coupon", "Self Scan",
                    "Bagging Area", "Receipt",
                ],
                media: [BridgeMediaCount(medium: .digital, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-16",
                    title: "Floors 1-12",
                    artist: "Stairwell Echo",
                    year: 2018,
                ),
                tracks: [
                    "Lobby", "Ascent", "Landing", "Fire Door", "Roof Access",
                ],
                media: [BridgeMediaCount(medium: .vinyl, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-17",
                    title: "Your Number",
                    artist: "The Waiting Room",
                    year: 2021,
                ),
                tracks: [
                    "Take a Ticket", "Now Serving", "Please Wait",
                    "Next Window", "Closed",
                ],
                media: [BridgeMediaCount(medium: .cd, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-18",
                    title: "Back Page",
                    artist: "Newspaper Weather",
                    year: 2021,
                ),
                tracks: [
                    "Classifieds", "Obituaries", "Comics", "Crossword",
                    "Horoscope", "Editorial",
                ],
                media: [BridgeMediaCount(medium: .digital, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-19",
                    title: "Mission Control",
                    artist: "The Cold Equations",
                    year: 2021,
                ),
                tracks: [
                    "Countdown", "Ignition", "Max Q", "MECO", "Orbit Achieved",
                    "Houston",
                ],
                media: [BridgeMediaCount(medium: .cd, count: 1)]
            ),
            makeDetail(
                album: .init(
                    id: "a-20",
                    title: "Collated",
                    artist: "Copy Machine",
                    year: 2020,
                ),
                tracks: [
                    "Warm Up", "Paper Jam", "Toner Low", "Duplex", "Staple",
                    "Output Tray",
                ],
                media: [BridgeMediaCount(medium: .digital, count: 1)]
            ),
            makeDetailTwoPart(
                album: .init(
                    id: "a-21",
                    title: "Double Feature",
                    artist: "The Midnight Signal",
                    year: 2024,
                ),
                first: [
                    "Opening Night", "Silver Screen", "Intermission",
                    "Plot Twist",
                    "Closing Credits",
                ],
                second: [
                    "Deleted Scenes", "Alternate Ending", "Director's Cut",
                    "Blooper Reel",
                ],
                media: [BridgeMediaCount(medium: .vinyl, count: 1)]
            ),
            makeDetailTwoPart(
                album: .init(
                    id: "a-22",
                    title: "Collected Works",
                    artist: "The Archivists",
                    year: 2019,
                ),
                first: [
                    "Preface", "Chapter One", "Chapter Two", "Interlude",
                    "Chapter Three",
                    "Epilogue", "Marginalia", "Footnotes", "Glossary",
                    "Bibliography",
                ],
                second: [
                    "Appendix A", "Appendix B", "Index", "Errata", "Colophon",
                    "Addendum",
                    "Corrigenda", "Afterword", "About the Author",
                ],
                media: [BridgeMediaCount(medium: .cd, count: 1)]
            ),
            // Nine tracks over eight: the first disc splits into two columns,
            // the second is short enough to stay one.
            makeDetailTwoPart(
                album: .init(
                    id: "a-23",
                    title: "Field Notes",
                    artist: "Survey Party",
                    year: 2022,
                ),
                first: [
                    "Transect", "Quadrat", "Baseline", "Waypoint", "Contour",
                    "Datum", "Traverse", "Benchmark", "Cairn",
                ],
                second: [
                    "Levelling", "Plumb Line", "Sight Line", "Chainage",
                    "Offset", "Bearing", "Elevation", "Closure",
                ],
                media: [BridgeMediaCount(medium: .cd, count: 1)]
            ),
        ]

        static let albumDetails: [String: BridgeAlbumDetail] = Dictionary(
            uniqueKeysWithValues: albumDetailList.map { ($0.album.id, $0) }
        )

    }
#endif
