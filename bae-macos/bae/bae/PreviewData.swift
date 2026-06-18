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

    static let importWatchedFolder = BridgeWatchedFolder(
        path: "/Music/Downloads",
        name: "Downloads"
    )

    /// Shared preview ConfigStore. ConfigStore is a non-Sendable `@Observable`,
    /// so it needs `@MainActor` isolation to hold as a static.
    @MainActor
    static let configStore = ConfigStore(
        config: Config(
            bridge: BridgeConfig(
                libraryId: "lib-preview",
                libraryName: "Preview Library",
                libraryPath: "/preview",
                encryptionKeyStored: false,
                encryptionKeyFingerprint: nil,
                discogsTokenStatus: .notConfigured,
                discogsUsable: false,
                sync: nil
            )
        ),
        syncReady: false
    )

    /// Seeded ImportStore for the FolderImportTab whole-view preview — the
    /// watched folder plus every folder candidate. ImportStore is a non-Sendable
    /// `@Observable`, so it needs `@MainActor` isolation to hold as a static.
    @MainActor
    static let folderImportStore: ImportStore = {
        let s = ImportStore()
        s.watchedFolders = [importWatchedFolder]
        for candidate in folderCandidates {
            s.folderCandidates[candidate.key] = candidate
        }
        return s
    }()

    static let folderCandidates: [Candidate] = [
        BridgeFolderCandidate(
            folderPath: "/Music/Downloads/Album Title One",
            sourceFolderName: "Album Title One",
            watchedFolderPath: "/Music/Downloads",
            files: bridgeCandidateFiles,
            trackCount: 9,
            skipped: false,
            isAdded: false,
        ),
        BridgeFolderCandidate(
            folderPath: "/Music/Downloads/Album Title Two [Label CAT-002]",
            sourceFolderName: "Album Title Two",
            watchedFolderPath: "/Music/Downloads",
            files: bridgeCandidateFiles,
            trackCount: 12,
            // Skipped example — renders under the Skipped tab.
            skipped: true,
            isAdded: false,
        ),
        BridgeFolderCandidate(
            folderPath: "/Music/Downloads/Compilation Vol. 3",
            sourceFolderName: "Compilation Vol. 3",
            watchedFolderPath: "/Music/Downloads",
            files: bridgeCandidateFiles,
            trackCount: 15,
            skipped: false,
            isAdded: false,
        ),
        BridgeFolderCandidate(
            folderPath: "/Music/Downloads/EP Release",
            sourceFolderName: "EP Release",
            watchedFolderPath: "/Music/Downloads",
            files: bridgeCandidateFiles,
            trackCount: 5,
            skipped: false,
            isAdded: false,
        ),
        BridgeFolderCandidate(
            folderPath: "/Music/Downloads/Live Recording 2023",
            sourceFolderName: "Live Recording 2023",
            watchedFolderPath: "/Music/Downloads",
            files: bridgeCandidateFiles,
            trackCount: 18,
            // Added example (content-hash match) — renders under the Added tab.
            skipped: false,
            isAdded: true,
        ),
    ]
    .map(Candidate.init(bridge:))

    static let importStatuses: [String: ImportStatus] = [
        "/Music/Downloads/Compilation Vol. 3": .importing(
            progressPercent: 45,
            phase: nil,
            statusText: "Storing files..."
        ),
        // A completed import — tabs under Added via its import status.
        "/Music/Downloads/EP Release": .complete(
            albumId: "preview-album",
            releaseId: "preview-release"
        ),
    ]

    /// Folders that look like a release but failed validation — surface under
    /// the Skipped tab with a warning and reason.
    static let invalidCandidates: [BridgeInvalidCandidate] = [
        BridgeInvalidCandidate(
            folderPath: "/Music/Downloads/Broken Rip",
            sourceFolderName: "Broken Rip",
            watchedFolderPath: "/Music/Downloads",
            reason: "corrupt or zero-byte audio file: 03.flac"
        )
    ]

    private static func previewArtworkFile(
        name: String,
        size: UInt64,
        sizeLabel: String,
        localPath: String
    ) -> BridgeArtworkFile {
        BridgeArtworkFile(
            file: BridgeFileInfo(
                name: name,
                size: size,
                sizeLabel: sizeLabel,
                dirPrefix: nil,
                fileName: name,
                localPath: localPath
            ),
            coverChoice: BridgeCoverChoice(
                selection: .releaseImage(fileId: name),
                previewSource: .local(path: localPath),
                thumbnailSource: .local(path: localPath)
            )
        )
    }

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
            previewArtworkFile(
                name: "Front.png",
                size: 2_500_000,
                sizeLabel: "2 MB",
                localPath: "/tmp/fake/Front.png"
            ),
            previewArtworkFile(
                name: "Back.png",
                size: 1_800_000,
                sizeLabel: "2 MB",
                localPath: "/tmp/fake/Back.png"
            ),
            previewArtworkFile(
                name: "Matrix.png",
                size: 900_000,
                sizeLabel: "879 KB",
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
            defaultCover: nil,
        )
    }()

    static let releaseDetail: ImportReleaseDetail = ImportReleaseDetail(
        bridge: releaseDetailBridge
    )

    /// Editor seed for the confirming previews — the raw release edit produced
    /// from the exact-pressing choice over `releaseDetailBridge`.
    static let confirmEditValues: BridgeRawReleaseEdit =
        rawReleaseEditFromUserEdit(
            edit: shapeUserEditFromReleaseDetail(
                detail: releaseDetailBridge,
                choice: .exact(
                    releaseId: releaseDetailBridge.releaseId,
                    source: releaseDetailBridge.source,
                )
            ),
            trackIdPrefix: "import-track"
        )

    /// Per-track audio candidate (nine FLAC files) plus one cover image and two
    /// documents — the track-files counterpart to `candidateFiles` (CUE+FLAC).
    static let candidateFilesTracks = CandidateFiles(
        bridge: BridgeCandidateFiles(
            audio: .trackFiles(
                files: (1...9)
                    .map { i in
                        BridgeFileInfo(
                            name: "Track \(i).flac",
                            size: UInt64(35_000_000 + i * 2_000_000),
                            sizeLabel: "\(33 + i * 2) MB",
                            dirPrefix: nil,
                            fileName: "Track \(i).flac",
                            localPath: "/tmp/fake/Track \(i).flac",
                        )
                    }
            ),
            artwork: [
                previewArtworkFile(
                    name: "Front.png",
                    size: 2_500_000,
                    sizeLabel: "2 MB",
                    localPath: "/tmp/fake/Front.png"
                )
            ],
            documents: [
                BridgeFileInfo(
                    name: "info.log",
                    size: 6000,
                    sizeLabel: "6 KB",
                    dirPrefix: nil,
                    fileName: "info.log",
                    localPath: "/tmp/fake/info.log"
                ),
                BridgeFileInfo(
                    name: "notes.txt",
                    size: 1200,
                    sizeLabel: "1 KB",
                    dirPrefix: nil,
                    fileName: "notes.txt",
                    localPath: "/tmp/fake/notes.txt"
                ),
            ],
        )
    )

    // MARK: - Import search

    /// Two pressings of one release group — the exact-match results state.
    static let exactPressings: [BridgeMetadataResult] = [
        BridgeMetadataResult(
            source: .musicBrainz,
            releaseId: "rel-123",
            year: 1996,
            format: "CD",
            label: "Label Name",
            catalogNumber: "6006-2",
            country: "US",
        ),
        BridgeMetadataResult(
            source: .musicBrainz,
            releaseId: "rel-456",
            year: 1988,
            format: "CD",
            label: "Label Name",
            catalogNumber: "1871-2",
            country: "US",
        ),
    ]

    static let searchGroupExact = ReleaseGroup(
        bridge: BridgeReleaseGroup(
            id: "group-preview",
            title: "Album Title",
            artist: "Artist Name",
            coverArt: nil,
            sourceLabel: "MusicBrainz",
            groupUrl: "https://musicbrainz.org/release-group/group-preview",
            metaLabel: "1988 \u{2013} 1996 \u{00b7} 2 pressings",
            pressings: exactPressings,
        )
    )

    static let searchProvenanceExact: [String: ResultProvenance] = Dictionary(
        uniqueKeysWithValues: exactPressings.map {
            (
                $0.releaseId,
                ResultProvenance(
                    byDiscId: true,
                    byBarcode: false,
                    matchesCatalog: true
                )
            )
        }
    )

    /// Two distinct release groups — the manual-search results state.
    static let searchGroupsManual: [ReleaseGroup] = [
        ReleaseGroup(
            bridge: BridgeReleaseGroup(
                id: "grp-1",
                title: "Album Title One",
                artist: "Artist Name",
                coverArt: nil,
                sourceLabel: "MusicBrainz",
                groupUrl: "https://musicbrainz.org/release-group/grp-1",
                metaLabel: "1996 \u{00b7} 2 pressings",
                pressings: [
                    BridgeMetadataResult(
                        source: .musicBrainz,
                        releaseId: "rel-aaa",
                        year: 1996,
                        format: "CD",
                        label: "Label Name",
                        catalogNumber: "6006-2",
                        country: "US",
                    ),
                    BridgeMetadataResult(
                        source: .musicBrainz,
                        releaseId: "rel-bbb",
                        year: 1996,
                        format: "CD",
                        label: "Another Label",
                        catalogNumber: "AL-1234",
                        country: "JP",
                    ),
                ],
            )
        ),
        ReleaseGroup(
            bridge: BridgeReleaseGroup(
                id: "grp-2",
                title: "Album Title One (Remaster)",
                artist: "Artist Name",
                coverArt: nil,
                sourceLabel: "MusicBrainz",
                groupUrl: "https://musicbrainz.org/release-group/grp-2",
                metaLabel: "2005 \u{00b7} 1 pressing",
                pressings: [
                    BridgeMetadataResult(
                        source: .musicBrainz,
                        releaseId: "rel-ccc",
                        year: 2005,
                        format: "CD",
                        label: "Reissue Records",
                        catalogNumber: "RR-500",
                        country: "EU",
                    )
                ],
            )
        ),
    ]

    /// disc-id vs barcode candidate lists — the conflict results state.
    static let conflictDiscidResults: [MetadataResult] = [
        BridgeMetadataResult(
            source: .musicBrainz,
            releaseId: "rel-disc-1",
            year: 1996,
            format: "CD",
            label: "Label A",
            catalogNumber: "AAA-001",
            country: "US",
        )
    ]
    .map(MetadataResult.init(bridge:))

    static let conflictBarcodeResults: [MetadataResult] = [
        BridgeMetadataResult(
            source: .musicBrainz,
            releaseId: "rel-bar-1",
            year: 2001,
            format: "CD",
            label: "Label B",
            catalogNumber: "BBB-002",
            country: "JP",
        )
    ]
    .map(MetadataResult.init(bridge:))

    /// Settled OCR/text signals — catalogs plus cover free-text.
    static let settledSignals = Signals(
        text: .settled(
            catalogs: ["WPCR-80001"],
            freeText: [
                "Artist Name",
                "Album Title",
                "Label Records JP - WPCR-80001",
                "Recorded at Studio A",
                "Produced by Producer Name",
            ]
        )
    )

    /// Exact-match display state: disc-id found one group, catalog confirms it.
    static let searchStateFoundExact = ImportSearchState(
        identifyState: .found(
            group: searchGroupExact,
            libraryStatuses: [:],
            trackCount: 0,
            source: .discid,
            provenance: searchProvenanceExact,
        ),
        showManualSearch: false,
        error: nil,
        searchGroups: [],
        selectedReleaseId: nil,
        isSearching: false,
        hasSearched: false,
        isImporting: false,
        libraryStatuses: [:],
        discogsEnabled: true,
        signals: settledSignals,
        signalsToolbar: SignalsToolbar(signals: [
            ToolbarSignal(
                kind: .discId,
                role: .identity,
                value: "disc-hash",
                origin: .discToc,
                state: .found(count: 3),
                excluded: false
            ),
            ToolbarSignal(
                kind: .catalog,
                role: .filter,
                value: "WPCR-80001",
                origin: .folderName,
                state: .confirms(count: 1),
                excluded: false
            ),
        ]),
    )

    /// Manual-search display state: results listed, the form open.
    static let searchStateManual = ImportSearchState(
        identifyState: .found(
            group: searchGroupsManual[0],
            libraryStatuses: [:],
            trackCount: 0,
            source: .discid,
            provenance: [:],
        ),
        showManualSearch: true,
        error: nil,
        searchGroups: searchGroupsManual,
        selectedReleaseId: nil,
        isSearching: false,
        hasSearched: true,
        isImporting: false,
        libraryStatuses: [:],
        discogsEnabled: true,
        signals: settledSignals,
        signalsToolbar: SignalsToolbar(signals: [
            ToolbarSignal(
                kind: .discId,
                role: .identity,
                value: "disc-hash",
                origin: .discToc,
                state: .found(count: 2),
                excluded: false
            ),
            ToolbarSignal(
                kind: .catalog,
                role: .filter,
                value: "WPCR-80001",
                origin: .folderName,
                state: .confirms(count: 0),
                excluded: false
            ),
        ]),
    )

    /// Conflict display state: disc-id and barcode disagree on identity.
    static let searchStateConflict = ImportSearchState(
        identifyState: .conflict(
            discidResults: conflictDiscidResults,
            discidLibraryStatuses: [:],
            barcodeResults: conflictBarcodeResults,
            barcodeLibraryStatuses: [:],
            discidSourceLabel: "MusicBrainz",
            matchedBarcode: "5051961234567",
            trackCount: 11,
        ),
        showManualSearch: false,
        error: nil,
        searchGroups: [],
        selectedReleaseId: nil,
        isSearching: false,
        hasSearched: false,
        isImporting: false,
        libraryStatuses: [:],
        discogsEnabled: true,
        signals: nil,
        signalsToolbar: SignalsToolbar(signals: [
            ToolbarSignal(
                kind: .discId,
                role: .identity,
                value: "disc-hash",
                origin: .discToc,
                state: .found(count: 2),
                excluded: false
            ),
            ToolbarSignal(
                kind: .barcode,
                role: .identity,
                value: "5051961234567",
                origin: .artwork,
                state: .found(count: 3),
                excluded: false
            ),
        ]),
    )

    /// Auto-lookup in progress: disc-id looking up, barcode skipped.
    static let searchStateTriangulating = ImportSearchState(
        identifyState: .triangulating(
            discid: .lookingUp,
            barcode: .skipped,
        ),
        showManualSearch: false,
        error: nil,
        searchGroups: [],
        selectedReleaseId: nil,
        isSearching: false,
        hasSearched: false,
        isImporting: false,
        libraryStatuses: [:],
        discogsEnabled: false,
        signals: nil,
        signalsToolbar: SignalsToolbar(signals: [
            ToolbarSignal(
                kind: .discId,
                role: .identity,
                value: "disc-hash",
                origin: .discToc,
                state: .lookingUp,
                excluded: false
            ),
            ToolbarSignal(
                kind: .barcode,
                role: .identity,
                value: nil,
                origin: .artwork,
                state: .skipped,
                excluded: false
            ),
        ]),
    )

    /// Manual search after both signals came up empty.
    static let searchStateNotFound = ImportSearchState(
        identifyState: .notFoundAnywhere,
        showManualSearch: true,
        error: nil,
        searchGroups: [],
        selectedReleaseId: nil,
        isSearching: false,
        hasSearched: false,
        isImporting: false,
        libraryStatuses: [:],
        discogsEnabled: true,
        signals: nil,
        signalsToolbar: SignalsToolbar(signals: [
            ToolbarSignal(
                kind: .discId,
                role: .identity,
                value: "disc-hash",
                origin: .discToc,
                state: .noMatch,
                excluded: false
            ),
            ToolbarSignal(
                kind: .barcode,
                role: .identity,
                value: "5051961234567",
                origin: .artwork,
                state: .noMatch,
                excluded: false
            ),
        ]),
    )
}
