import Foundation

/// Shared sample data for SwiftUI #Preview blocks.
/// Album/artist/track names match bae-mocks/fixtures/data.json.
enum PreviewData {
    // MARK: - Albums

    static let albums: [BridgeAlbum] = [
        BridgeAlbum(
            id: "a-01",
            title: "Neon Frequencies",
            year: 2023,
            isCompilation: false,
            artistNames: "The Midnight Signal",
            releaseIds: ["r-01"],
            primaryReleaseId: "r-01",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-02",
            title: "Pacific Standard",
            year: 2019,
            isCompilation: false,
            artistNames: "Glass Harbor",
            releaseIds: ["r-02"],
            primaryReleaseId: "r-02",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-03",
            title: "Proof by Induction",
            year: 2021,
            isCompilation: false,
            artistNames: "Velvet Mathematics",
            releaseIds: ["r-03"],
            primaryReleaseId: "r-03",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-04",
            title: "Seconds",
            year: 1974,
            isCompilation: false,
            artistNames: "The Borrowed Time",
            releaseIds: ["r-04", "r-04b"],
            primaryReleaseId: "r-04",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-05",
            title: "Window Sill",
            year: 2020,
            isCompilation: false,
            artistNames: "Apartment Garden",
            releaseIds: ["r-05"],
            primaryReleaseId: "r-05",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-06",
            title: "Fuel Weight",
            year: 2018,
            isCompilation: false,
            artistNames: "The Cold Equations",
            releaseIds: ["r-06"],
            primaryReleaseId: "r-06",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-07",
            title: "Tomorrow's Forecast",
            year: 2023,
            isCompilation: false,
            artistNames: "Newspaper Weather",
            releaseIds: ["r-07"],
            primaryReleaseId: "r-07",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-08",
            title: "Alphabetical",
            year: 2017,
            isCompilation: false,
            artistNames: "The Filing Cabinets",
            releaseIds: ["r-08"],
            primaryReleaseId: "r-08",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-09",
            title: "Level 4",
            year: 2021,
            isCompilation: false,
            artistNames: "Parking Structure",
            releaseIds: ["r-09"],
            primaryReleaseId: "r-09",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-10",
            title: "Dial Tone",
            year: 2019,
            isCompilation: false,
            artistNames: "The Last Payphone",
            releaseIds: ["r-10"],
            primaryReleaseId: "r-10",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-11",
            title: "Set Theory",
            year: 2019,
            isCompilation: false,
            artistNames: "Velvet Mathematics",
            releaseIds: ["r-11"],
            primaryReleaseId: "r-11",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-12",
            title: "Interest",
            year: 2020,
            isCompilation: false,
            artistNames: "The Borrowed Time",
            releaseIds: ["r-12"],
            primaryReleaseId: "r-12",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-13",
            title: "Grow Light",
            year: 2022,
            isCompilation: false,
            artistNames: "Apartment Garden",
            releaseIds: ["r-13"],
            primaryReleaseId: "r-13",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-14",
            title: "Landlocked",
            year: 2022,
            isCompilation: false,
            artistNames: "Glass Harbor",
            releaseIds: ["r-14"],
            primaryReleaseId: "r-14",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-15",
            title: "Express",
            year: 2023,
            isCompilation: false,
            artistNames: "The Checkout Lane",
            releaseIds: ["r-15"],
            primaryReleaseId: "r-15",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-16",
            title: "Floors 1-12",
            year: 2018,
            isCompilation: false,
            artistNames: "Stairwell Echo",
            releaseIds: ["r-16"],
            primaryReleaseId: "r-16",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-17",
            title: "Your Number",
            year: 2021,
            isCompilation: false,
            artistNames: "The Waiting Room",
            releaseIds: ["r-17"],
            primaryReleaseId: "r-17",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-18",
            title: "Back Page",
            year: 2021,
            isCompilation: false,
            artistNames: "Newspaper Weather",
            releaseIds: ["r-18"],
            primaryReleaseId: "r-18",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-19",
            title: "Mission Control",
            year: 2021,
            isCompilation: false,
            artistNames: "The Cold Equations",
            releaseIds: ["r-19"],
            primaryReleaseId: "r-19",
            coverPath: nil
        ),
        BridgeAlbum(
            id: "a-20",
            title: "Collated",
            year: 2020,
            isCompilation: false,
            artistNames: "Copy Machine",
            releaseIds: ["r-20"],
            primaryReleaseId: "r-20",
            coverPath: nil
        ),
    ]

    // MARK: - Queue

    static let queueItems: [QueueItem] = [
        BridgeQueueItem(
            trackId: "t-01",
            title: "Static Dreams",
            artistNames: "The Midnight Signal",
            durationMs: 210_000,
            durationLabel: "3:30",
            albumTitle: "Neon Frequencies",
            coverImageId: nil
        ),
        BridgeQueueItem(
            trackId: "t-02",
            title: "Frequency Drift",
            artistNames: "The Midnight Signal",
            durationMs: 240_000,
            durationLabel: "4:00",
            albumTitle: "Neon Frequencies",
            coverImageId: nil
        ),
        BridgeQueueItem(
            trackId: "t-03",
            title: "Tide Pool",
            artistNames: "Glass Harbor",
            durationMs: 198_000,
            durationLabel: "3:18",
            albumTitle: "Pacific Standard",
            coverImageId: nil
        ),
        BridgeQueueItem(
            trackId: "t-04",
            title: "Harbor Lights",
            artistNames: "Glass Harbor",
            durationMs: 225_000,
            durationLabel: "3:45",
            albumTitle: "Pacific Standard",
            coverImageId: nil
        ),
        BridgeQueueItem(
            trackId: "t-05",
            title: "Axiom",
            artistNames: "Velvet Mathematics",
            durationMs: 187_000,
            durationLabel: "3:07",
            albumTitle: "Proof by Induction",
            coverImageId: nil
        ),
    ]
    .map(QueueItem.init(bridge:))

    // MARK: - Now Playing

    static let nowPlayingTitle = "Broadcast"
    static let nowPlayingArtist = "The Midnight Signal"

    // MARK: - Album Details

    private static func makeTracks(
        _ names: [String],
        artist: String,
        side: Int32 = 1,
        sideLabel: String = "",
        positionPrefix: String = "",
    ) -> [BridgeTrack] {
        names.enumerated()
            .map { index, name in
                let durationMs = Int64((170 + (index * 37) % 170) * 1000)
                let totalSeconds = durationMs / 1000
                let minutes = totalSeconds / 60
                let seconds = totalSeconds % 60
                let durationLabel =
                    "\(minutes):\(String(format: "%02d", seconds))"
                let posLabel =
                    positionPrefix.isEmpty
                    ? "\(index + 1)" : "\(positionPrefix)\(index + 1)"
                return BridgeTrack(
                    id: "t-d\(side)-\(index + 1)",
                    title: name,
                    side: side,
                    trackNumber: Int32(index + 1),
                    durationMs: durationMs,
                    durationLabel: durationLabel,
                    artistNames: artist,
                    sideLabel: sideLabel,
                    positionLabel: posLabel,
                )
            }
    }

    private static func makeDetail(
        id: String,
        title: String,
        artist: String,
        year: Int32,
        tracks: [String],
        format: String,
    ) -> BridgeAlbumDetail {
        BridgeAlbumDetail(
            album: BridgeAlbum(
                id: id,
                title: title,
                year: year,
                isCompilation: false,
                artistNames: artist,
                releaseIds: ["rel-\(id)"],
                primaryReleaseId: "rel-\(id)",
                coverPath: nil,
            ),
            releases: [
                BridgeRelease(
                    id: "rel-\(id)",
                    albumId: id,
                    displayName: "\(year) \(format)",
                    releaseName: nil,
                    year: year,
                    format: format,
                    label: nil,
                    catalogNumber: nil,
                    country: nil,
                    compactMetadata: "\(year) \u{00B7} \(format)",
                    storageState: .unmanaged,
                    storageActions: [],
                    tracks: makeTracks(tracks, artist: artist),
                    trackGroups: [
                        BridgeTrackGroup(
                            sideLabel: "",
                            tracks: makeTracks(tracks, artist: artist)
                        )
                    ],
                    files: [],
                    imageFiles: [],
                    galleryItems: [],
                    totalDurationLabel: "39 min",
                    fileCount: 0,
                    totalSize: 0,
                    totalSizeLabel: "0 bytes",
                    coverPath: nil,
                )
            ],
        )
    }

    private static func makeDetailTwoDisc(
        id: String,
        title: String,
        artist: String,
        year: Int32,
        disc1: [String],
        disc2: [String],
        format: String,
    ) -> BridgeAlbumDetail {
        let isVinyl = format.contains("Vinyl") || format.contains("Cassette")
        let allTracks =
            makeTracks(
                disc1,
                artist: artist,
                side: 1,
                sideLabel: isVinyl ? "Side A" : "Disc 1",
                positionPrefix: isVinyl ? "A" : "",
            )
            + makeTracks(
                disc2,
                artist: artist,
                side: 2,
                sideLabel: isVinyl ? "Side B" : "Disc 2",
                positionPrefix: isVinyl ? "B" : "",
            )
        return BridgeAlbumDetail(
            album: BridgeAlbum(
                id: id,
                title: title,
                year: year,
                isCompilation: false,
                artistNames: artist,
                releaseIds: ["rel-\(id)"],
                primaryReleaseId: "rel-\(id)",
                coverPath: nil,
            ),
            releases: [
                BridgeRelease(
                    id: "rel-\(id)",
                    albumId: id,
                    displayName: "\(year) \(format)",
                    releaseName: nil,
                    year: year,
                    format: format,
                    label: "Some Label",
                    catalogNumber: "CAT-001",
                    country: "US",
                    compactMetadata:
                        "\(year) \u{00B7} \(format) \u{00B7} Some Label \u{00B7} CAT-001 \u{00B7} US",
                    storageState: .unmanaged,
                    storageActions: [],
                    tracks: allTracks,
                    trackGroups: [
                        BridgeTrackGroup(sideLabel: "", tracks: allTracks)
                    ],
                    files: [],
                    imageFiles: [],
                    galleryItems: [],
                    totalDurationLabel: "39 min",
                    fileCount: 0,
                    totalSize: 0,
                    totalSizeLabel: "0 bytes",
                    coverPath: nil,
                )
            ],
        )
    }

    private static func makeDetailMultiRelease(
        id: String,
        title: String,
        artist: String,
        year: Int32,
        releases: [(
            releaseName: String?, year: Int32, format: String, tracks: [String],
            disc2: [String]?
        )],
    ) -> BridgeAlbumDetail {
        let releaseIds = releases.indices.map { "rel-\(id)-\($0)" }
        let bridgeReleases = releases.enumerated()
            .map { index, rel in
                let displayName =
                    rel.releaseName
                    ?? [String(rel.year), rel.format].joined(separator: " ")
                let isVinyl =
                    rel.format.contains("Vinyl")
                    || rel.format.contains("Cassette")
                let allTracks: [BridgeTrack] =
                    if let disc2 = rel.disc2 {
                        makeTracks(
                            rel.tracks,
                            artist: artist,
                            side: 1,
                            sideLabel: isVinyl ? "Side A" : "Disc 1",
                            positionPrefix: isVinyl ? "A" : "",
                        )
                            + makeTracks(
                                disc2,
                                artist: artist,
                                side: 2,
                                sideLabel: isVinyl ? "Side B" : "Disc 2",
                                positionPrefix: isVinyl ? "B" : "",
                            )
                    }
                    else {
                        makeTracks(rel.tracks, artist: artist)
                    }
                return BridgeRelease(
                    id: releaseIds[index],
                    albumId: id,
                    displayName: displayName,
                    releaseName: rel.releaseName,
                    year: rel.year,
                    format: rel.format,
                    label: nil,
                    catalogNumber: nil,
                    country: nil,
                    compactMetadata: "\(rel.year) \u{00B7} \(rel.format)",
                    storageState: .unmanaged,
                    storageActions: [],
                    tracks: allTracks,
                    trackGroups: [
                        BridgeTrackGroup(sideLabel: "", tracks: allTracks)
                    ],
                    files: [],
                    imageFiles: [],
                    galleryItems: [],
                    totalDurationLabel: "39 min",
                    fileCount: 0,
                    totalSize: 0,
                    totalSizeLabel: "0 bytes",
                    coverPath: nil,
                )
            }
        return BridgeAlbumDetail(
            album: BridgeAlbum(
                id: id,
                title: title,
                year: year,
                isCompilation: false,
                artistNames: artist,
                releaseIds: releaseIds,
                primaryReleaseId: releaseIds[0],
                coverPath: nil,
            ),
            releases: bridgeReleases,
        )
    }

    static let albumDetails: [String: BridgeAlbumDetail] = {
        let details = [
            makeDetail(
                id: "a-01",
                title: "Neon Frequencies",
                artist: "The Midnight Signal",
                year: 2023,
                tracks: [
                    "Broadcast", "Static Dreams", "Frequency Drift",
                    "Night Transmission",
                    "Signal Lost", "Airwave", "Carrier Wave", "Sign Off",
                ],
                format: "Digital"
            ),
            makeDetail(
                id: "a-02",
                title: "Pacific Standard",
                artist: "Glass Harbor",
                year: 2019,
                tracks: [
                    "Coastal", "Tide Pool", "Harbor Lights", "Salt Air",
                    "Driftwood", "Fog Horn",
                    "Pier 17", "Last Ferry",
                ],
                format: "Vinyl"
            ),
            makeDetail(
                id: "a-03",
                title: "Proof by Induction",
                artist: "Velvet Mathematics",
                year: 2021,
                tracks: [
                    "Axiom", "Recursive", "Limit Theorem", "Derivative",
                    "Integral", "Convergence",
                    "QED",
                ],
                format: "CD"
            ),
            makeDetailMultiRelease(
                id: "a-04",
                title: "Seconds",
                artist: "The Borrowed Time",
                year: 1974,
                releases: [
                    (
                        releaseName: "1974 Vinyl", year: 1974, format: "Vinyl",
                        tracks: [
                            "Tick", "Borrowed", "Overdue", "Extension",
                            "Final Notice",
                            "Grace Period",
                        ], disc2: nil
                    ),
                    (
                        releaseName: "1996 Reissue", year: 1996, format: "2xCD",
                        tracks: [
                            "Tick", "Borrowed", "Overdue", "Extension",
                            "Final Notice",
                            "Grace Period",
                        ],
                        disc2: [
                            "Overtime", "Second Chance", "Borrowed (Demo)",
                            "Final Notice (Live)",
                        ]
                    ),
                ]
            ),
            makeDetail(
                id: "a-05",
                title: "Window Sill",
                artist: "Apartment Garden",
                year: 2020,
                tracks: [
                    "Basil", "Morning Light", "Terracotta", "Propagation",
                    "Root Bound",
                    "Water Day", "New Growth",
                ],
                format: "Digital"
            ),
            makeDetail(
                id: "a-06",
                title: "Fuel Weight",
                artist: "The Cold Equations",
                year: 2018,
                tracks: [
                    "Launch Window", "Trajectory", "Orbital Decay", "Reentry",
                    "Terminal Velocity",
                    "Escape", "Gravity Well",
                ],
                format: "Vinyl"
            ),
            makeDetail(
                id: "a-07",
                title: "Tomorrow's Forecast",
                artist: "Newspaper Weather",
                year: 2023,
                tracks: [
                    "Partly Cloudy", "High Pressure", "Cold Front",
                    "Scattered Showers",
                    "Clearing Skies", "Weekend Outlook",
                ],
                format: "Digital"
            ),
            makeDetail(
                id: "a-08",
                title: "Alphabetical",
                artist: "The Filing Cabinets",
                year: 2017,
                tracks: [
                    "A-D", "E-H", "I-L", "M-P", "Q-T", "U-Z", "Miscellaneous",
                ],
                format: "CD"
            ),
            makeDetail(
                id: "a-09",
                title: "Level 4",
                artist: "Parking Structure",
                year: 2021,
                tracks: [
                    "Entrance", "Spiral Up", "Compact Only", "Reserved",
                    "Exit Ticket",
                    "Night Rate",
                ],
                format: "Digital"
            ),
            makeDetail(
                id: "a-10",
                title: "Dial Tone",
                artist: "The Last Payphone",
                year: 2019,
                tracks: [
                    "Insert Coin", "Area Code", "Long Distance", "Collect Call",
                    "Busy Signal",
                    "Disconnected",
                ],
                format: "Cassette"
            ),
            makeDetail(
                id: "a-11",
                title: "Set Theory",
                artist: "Velvet Mathematics",
                year: 2019,
                tracks: [
                    "Union", "Intersection", "Complement", "Subset",
                    "Empty Set", "Cardinality",
                ],
                format: "CD"
            ),
            makeDetail(
                id: "a-12",
                title: "Interest",
                artist: "The Borrowed Time",
                year: 2020,
                tracks: [
                    "Principal", "Compound", "Balloon Payment", "Amortization",
                    "Default",
                    "Refinance",
                ],
                format: "Digital"
            ),
            makeDetail(
                id: "a-13",
                title: "Grow Light",
                artist: "Apartment Garden",
                year: 2022,
                tracks: [
                    "Spectrum", "Photosynthesis", "Chlorophyll", "Dormancy",
                    "Spring Bloom",
                    "Perennial",
                ],
                format: "Vinyl"
            ),
            makeDetail(
                id: "a-14",
                title: "Landlocked",
                artist: "Glass Harbor",
                year: 2022,
                tracks: [
                    "Dry Dock", "Anchor", "Barnacles", "Rust", "Restoration",
                    "Launch Day",
                    "Open Water",
                ],
                format: "CD"
            ),
            makeDetail(
                id: "a-15",
                title: "Express",
                artist: "The Checkout Lane",
                year: 2023,
                tracks: [
                    "15 Items", "Price Check", "Coupon", "Self Scan",
                    "Bagging Area", "Receipt",
                ],
                format: "Digital"
            ),
            makeDetail(
                id: "a-16",
                title: "Floors 1-12",
                artist: "Stairwell Echo",
                year: 2018,
                tracks: [
                    "Lobby", "Ascent", "Landing", "Fire Door", "Roof Access",
                ],
                format: "Vinyl"
            ),
            makeDetail(
                id: "a-17",
                title: "Your Number",
                artist: "The Waiting Room",
                year: 2021,
                tracks: [
                    "Take a Ticket", "Now Serving", "Please Wait",
                    "Next Window", "Closed",
                ],
                format: "CD"
            ),
            makeDetail(
                id: "a-18",
                title: "Back Page",
                artist: "Newspaper Weather",
                year: 2021,
                tracks: [
                    "Classifieds", "Obituaries", "Comics", "Crossword",
                    "Horoscope", "Editorial",
                ],
                format: "Digital"
            ),
            makeDetail(
                id: "a-19",
                title: "Mission Control",
                artist: "The Cold Equations",
                year: 2021,
                tracks: [
                    "Countdown", "Ignition", "Max Q", "MECO", "Orbit Achieved",
                    "Houston",
                ],
                format: "CD"
            ),
            makeDetail(
                id: "a-20",
                title: "Collated",
                artist: "Copy Machine",
                year: 2020,
                tracks: [
                    "Warm Up", "Paper Jam", "Toner Low", "Duplex", "Staple",
                    "Output Tray",
                ],
                format: "Digital"
            ),
            makeDetailTwoDisc(
                id: "a-21",
                title: "Double Feature",
                artist: "The Midnight Signal",
                year: 2024,
                disc1: [
                    "Opening Night", "Silver Screen", "Intermission",
                    "Plot Twist",
                    "Closing Credits",
                ],
                disc2: [
                    "Deleted Scenes", "Alternate Ending", "Director's Cut",
                    "Blooper Reel",
                ],
                format: "12\" Vinyl"
            ),
            makeDetailTwoDisc(
                id: "a-22",
                title: "Collected Works",
                artist: "The Archivists",
                year: 2019,
                disc1: [
                    "Preface", "Chapter One", "Chapter Two", "Interlude",
                    "Chapter Three",
                    "Epilogue", "Marginalia", "Footnotes", "Glossary",
                    "Bibliography",
                ],
                disc2: [
                    "Appendix A", "Appendix B", "Index", "Errata", "Colophon",
                    "Addendum",
                    "Corrigenda", "Afterword", "About the Author",
                ],
                format: "CD"
            ),
        ]
        return Dictionary(
            uniqueKeysWithValues: details.map { ($0.album.id, $0) }
        )
    }()

    // MARK: - Import

    static let folderCandidates: [Candidate] = [
        BridgeFolderCandidate(
            folderPath: "/Music/Downloads/Album Title One",
            sourceFolderName: "Album Title One",
            files: bridgeCandidateFiles,
            trackCount: 9,
        ),
        BridgeFolderCandidate(
            folderPath: "/Music/Downloads/Album Title Two [Label CAT-002]",
            sourceFolderName: "Album Title Two",
            files: bridgeCandidateFiles,
            trackCount: 12,
        ),
        BridgeFolderCandidate(
            folderPath: "/Music/Downloads/Compilation Vol. 3",
            sourceFolderName: "Compilation Vol. 3",
            files: bridgeCandidateFiles,
            trackCount: 15,
        ),
        BridgeFolderCandidate(
            folderPath: "/Music/Downloads/EP Release",
            sourceFolderName: "EP Release",
            files: bridgeCandidateFiles,
            trackCount: 5,
        ),
        BridgeFolderCandidate(
            folderPath: "/Music/Downloads/Live Recording 2023",
            sourceFolderName: "Live Recording 2023",
            files: bridgeCandidateFiles,
            trackCount: 18,
        ),
    ]
    .map(Candidate.init(bridge:))

    static let importStatuses: [String: ImportStatus] = [
        "/Music/Downloads/Compilation Vol. 3": .importing(
            progressPercent: 45,
            phase: nil,
            statusText: "Storing files..."
        ),
        "/Music/Downloads/Live Recording 2023": .importing(
            progressPercent: 30,
            phase: "acquire",
            statusText: "Downloading..."
        ),
        "/Music/Downloads/EP Release": .complete(
            albumId: "preview-album",
            releaseId: "preview-release"
        ),
    ]

    static let bridgeCandidateFiles = BridgeCandidateFiles(
        audio: .cueFlacPairs(pairs: [
            BridgeCueFlacPair(
                cueName: "Album Title.cue",
                cueSize: 1200,
                cueSizeLabel: "1 KB",
                cueLocalPath: "/tmp/fake/Album Title.cue",
                flacName: "Album Title.flac",
                flacLocalPath: "/tmp/fake/Album Title.flac",
                totalSize: 340_000_000,
                totalSizeLabel: "324 MB",
                trackCount: 9,
            )
        ]),
        artwork: [
            BridgeFileInfo(
                name: "Front.png",
                size: 2_500_000,
                sizeLabel: "2 MB",
                dirPrefix: nil,
                fileName: "Front.png",
                localPath: "/tmp/fake/Front.png"
            ),
            BridgeFileInfo(
                name: "Back.png",
                size: 1_800_000,
                sizeLabel: "2 MB",
                dirPrefix: nil,
                fileName: "Back.png",
                localPath: "/tmp/fake/Back.png"
            ),
            BridgeFileInfo(
                name: "Matrix.png",
                size: 900_000,
                sizeLabel: "879 KB",
                dirPrefix: nil,
                fileName: "Matrix.png",
                localPath: "/tmp/fake/Matrix.png"
            ),
        ],
        documents: [
            BridgeFileInfo(
                name: "info.log",
                size: 6000,
                sizeLabel: "6 KB",
                dirPrefix: nil,
                fileName: "info.log",
                localPath: "/tmp/fake/info.log"
            )
        ],
    )

    static let candidateFiles = CandidateFiles(bridge: bridgeCandidateFiles)

    static let releaseDetailBridge: BridgeReleaseDetail = {
        let tracks: [BridgeReleaseTrack] = (1...9)
            .map { i in
                let ms = UInt64(180_000 + i * 15000)
                let totalSeconds = ms / 1000
                let minutes = totalSeconds / 60
                let seconds = totalSeconds % 60
                let durationLabel =
                    "\(minutes):\(String(format: "%02d", seconds))"
                return BridgeReleaseTrack(
                    title: "Track Title \(i)",
                    artist: i == 5 ? "Featured Artist" : nil,
                    durationMs: ms,
                    durationLabel: durationLabel,
                    position: "\(i)",
                    side: 1,
                    sideLabel: "",
                    positionLabel: "\(i)",
                )
            }
        return BridgeReleaseDetail(
            releaseId: "rel-123",
            source: .musicBrainz,
            sourceGroupId: "rg-123",
            title: "Album Title One",
            artist: "Artist Name",
            year: 1996,
            format: "CD",
            label: "Label Name",
            catalogNumber: "6006-2",
            country: "US",
            barcode: nil,
            trackCount: 9,
            trackCountMismatch: false,
            tracks: tracks,
            coverArt: [],
            defaultCoverUrl: nil,
        )
    }()

    static let releaseDetail: ImportReleaseDetail = ImportReleaseDetail(
        bridge: releaseDetailBridge
    )
}
