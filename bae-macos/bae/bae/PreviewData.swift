import Foundation

/// Shared sample data for SwiftUI #Preview blocks.
/// Album/artist/track names match bae-mocks/fixtures/data.json.
enum PreviewData {
    // MARK: - Albums

    /// The grid list, derived from the detail payloads so the grid summaries and
    /// the seeded `releaseDetails` agree on release ids. Ordered a-01..a-22,
    /// matching `albumDetailList`.
    static let albums: [BridgeAlbum] = albumDetailList.map(\.album)

    // MARK: - Queue

    static let queueItems: [QueueItem] = [
        BridgeQueueEntry(
            entryId: "e-01",
            trackId: "t-01",
            title: "Track Title 1",
            artistNames: "Artist Name A",
            durationMs: 210_000,
            albumTitle: "Album Title A",
            coverImageId: nil
        ),
        BridgeQueueEntry(
            entryId: "e-02",
            trackId: "t-02",
            title: "Track Title 2",
            artistNames: "Artist Name A",
            durationMs: 240_000,
            albumTitle: "Album Title A",
            coverImageId: nil
        ),
        BridgeQueueEntry(
            entryId: "e-03",
            trackId: "t-03",
            title: "Track Title 3",
            artistNames: "Artist Name B",
            durationMs: 198_000,
            albumTitle: "Album Title B",
            coverImageId: nil
        ),
        BridgeQueueEntry(
            entryId: "e-04",
            trackId: "t-04",
            title: "Track Title 4",
            artistNames: "Artist Name B",
            durationMs: 225_000,
            albumTitle: "Album Title B",
            coverImageId: nil
        ),
        BridgeQueueEntry(
            entryId: "e-05",
            trackId: "t-05",
            title: "Track Title 5",
            artistNames: "Artist Name C",
            durationMs: 187_000,
            albumTitle: "Album Title C",
            coverImageId: nil
        ),
    ]
    .map(QueueItem.init(bridge:))

    // MARK: - Now Playing

    static let nowPlayingTitle = "Broadcast"
    static let nowPlayingArtist = "The Midnight Signal"

    // MARK: - Search

    /// Search results — three album hits and two track hits — for the
    /// SearchView "With results" preview.
    static let searchResults = SearchResults(
        bridge: BridgeSearchResults(
            albums: [
                BridgeAlbumSearchResult(
                    id: "a-02",
                    title: "Pacific Standard",
                    year: 2019,
                    primaryReleaseId: "r-02",
                    artistName: "Glass Harbor"
                ),
                BridgeAlbumSearchResult(
                    id: "a-14",
                    title: "Landlocked",
                    year: 2022,
                    primaryReleaseId: "r-14",
                    artistName: "Glass Harbor"
                ),
                BridgeAlbumSearchResult(
                    id: "a-03",
                    title: "Proof by Induction",
                    year: 2021,
                    primaryReleaseId: "r-03",
                    artistName: "Velvet Mathematics"
                ),
            ],
            tracks: [
                BridgeTrackSearchResult(
                    id: "t-03",
                    title: "Tide Pool",
                    durationMs: 198_000,
                    albumId: "a-02",
                    albumTitle: "Pacific Standard",
                    artistName: "Glass Harbor"
                ),
                BridgeTrackSearchResult(
                    id: "t-05",
                    title: "Axiom",
                    durationMs: 187_000,
                    albumId: "a-03",
                    albumTitle: "Proof by Induction",
                    artistName: "Velvet Mathematics"
                ),
            ],
        ),
        query: "glass"
    )

    // MARK: - Cover sheet

    /// Remote cover candidates (front + back) for the CoverSheetView preview.
    static let remoteCovers: [BridgeRemoteCover] = [
        remoteCover(
            url: "https://example.com/cover1.jpg",
            thumbnailUrl: "https://example.com/thumb1.jpg",
            label: "Front"
        ),
        remoteCover(
            url: "https://example.com/cover2.jpg",
            thumbnailUrl: "https://example.com/thumb2.jpg",
            label: "Back"
        ),
    ]

    private static func remoteCover(
        url: String,
        thumbnailUrl: String,
        label: String
    ) -> BridgeRemoteCover {
        BridgeRemoteCover(
            coverChoice: BridgeCoverChoice(
                selection: .remoteCover(
                    selection: BridgeRemoteCoverSelection(
                        url: url,
                        source: .musicBrainz
                    )
                ),
                previewSource: .remote(url: url),
                thumbnailSource: .remote(url: thumbnailUrl)
            ),
            label: label
        )
    }

    // MARK: - Album Details

    private static func makeTracks(
        _ names: [String],
        artist: String,
        side: Int32 = 1,
        position: (Int) -> BridgeTrackPosition = {
            .flat(number: Int32($0 + 1))
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
                    artistNames: artist,
                    position: position(index),
                )
            }
    }

    /// A two-part release's tracks plus the matching per-side groups, so the
    /// preview renders real "Side A" / "Disc 2" headers. Vinyl sides get
    /// `.sided` with the letter supplied (A/B); digital gets `.disc` (1/2).
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
            let tracks = makeTracks(names, artist: artist, side: sideNumber) {
                index in
                isVinyl
                    ? .sided(sideLetter: letter, number: Int32(index + 1))
                    : .disc(disc: sideNumber, number: Int32(index + 1))
            }
            let groupSide: BridgeTrackSide =
                isVinyl ? .sided(sideLetter: letter) : .disc(disc: sideNumber)
            return (tracks, BridgeTrackGroup(side: groupSide, tracks: tracks))
        }
        let (tracks1, group1) = side(1, letter: "A", names: disc1)
        let (tracks2, group2) = side(2, letter: "B", names: disc2)
        return (tracks1 + tracks2, [group1, group2])
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
                    storageState: .local,
                    pinned: false,
                    storageActions: [],
                    tracks: makeTracks(tracks, artist: artist),
                    trackGroups: [
                        BridgeTrackGroup(
                            side: .flat,
                            tracks: makeTracks(tracks, artist: artist)
                        )
                    ],
                    files: [],
                    imageFiles: [],
                    galleryItems: [],
                    totalDurationMs: 2_340_000,
                    fileCount: 0,
                    totalSize: 0,
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
        let sides = twoSide(
            artist: artist,
            isVinyl: isVinyl,
            disc1: disc1,
            disc2: disc2,
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
                    storageState: .local,
                    pinned: false,
                    storageActions: [],
                    tracks: sides.tracks,
                    trackGroups: sides.groups,
                    files: [],
                    imageFiles: [],
                    galleryItems: [],
                    totalDurationMs: 2_340_000,
                    fileCount: 0,
                    totalSize: 0,
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
                let allTracks: [BridgeTrack]
                let groups: [BridgeTrackGroup]
                if let disc2 = rel.disc2 {
                    let sides = twoSide(
                        artist: artist,
                        isVinyl: isVinyl,
                        disc1: rel.tracks,
                        disc2: disc2,
                    )
                    allTracks = sides.tracks
                    groups = sides.groups
                }
                else {
                    allTracks = makeTracks(rel.tracks, artist: artist)
                    groups = [
                        BridgeTrackGroup(
                            side: .flat,
                            tracks: allTracks
                        )
                    ]
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
                    storageState: .local,
                    pinned: false,
                    storageActions: [],
                    tracks: allTracks,
                    trackGroups: groups,
                    files: [],
                    imageFiles: [],
                    galleryItems: [],
                    totalDurationMs: 2_340_000,
                    fileCount: 0,
                    totalSize: 0,
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

    /// The ordered album payloads (a-01..a-22). The single source of truth:
    /// `albums` is `albumDetailList.map(\.album)` and `albumDetails` is keyed by
    /// `album.id`, so the grid summaries and the seeded `releaseDetails` always
    /// agree on release ids.
    static let albumDetailList: [BridgeAlbumDetail] = [
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

    static let albumDetails: [String: BridgeAlbumDetail] = Dictionary(
        uniqueKeysWithValues: albumDetailList.map { ($0.album.id, $0) }
    )

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
                pauseBetweenSides: false,
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
            step: .running(phase: .store)
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
            reason: .corruptAudioFile(path: "03.flac")
        )
    ]

    private static func previewArtworkFile(
        name: String,
        size: UInt64,
        localPath: String
    ) -> BridgeArtworkFile {
        BridgeArtworkFile(
            file: BridgeFileInfo(
                name: name,
                size: size,
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
                cueLocalPath: "/tmp/fake/Album Title.cue",
                flacName: "Album Title.flac",
                flacLocalPath: "/tmp/fake/Album Title.flac",
                totalSize: 340_000_000,
                trackCount: 9,
            )
        ]),
        artwork: [
            previewArtworkFile(
                name: "Front.png",
                size: 2_500_000,
                localPath: "/tmp/fake/Front.png"
            ),
            previewArtworkFile(
                name: "Back.png",
                size: 1_800_000,
                localPath: "/tmp/fake/Back.png"
            ),
            previewArtworkFile(
                name: "Matrix.png",
                size: 900_000,
                localPath: "/tmp/fake/Matrix.png"
            ),
        ],
        documents: [
            BridgeFileInfo(
                name: "info.log",
                size: 6000,
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
                return BridgeReleaseTrack(
                    title: "Track Title \(i)",
                    artist: i == 5 ? "Featured Artist" : nil,
                    durationMs: ms,
                    position: "\(i)",
                    side: 1,
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

    /// A raw release edit for the EditMetadataForm / EditMetadataSheet previews:
    /// the album + pressing fields plus `trackCount` tracks. Blank track artists
    /// (the default) exercise the "track artist falls back to album artist"
    /// placeholder path the form renders — both previews wrap that form.
    static func editMetadataSeed(
        trackCount: Int,
        blankTrackArtists: Bool = true
    ) -> BridgeRawReleaseEdit {
        BridgeRawReleaseEdit(
            albumTitle: "Album Title",
            albumArtistText: "Artist Name",
            pressing: BridgeRawPressingEdit(
                year: "1997",
                format: "CD",
                label: "Some Label",
                catalogNumber: "CAT-0001",
                country: "US",
                barcode: "000000000000"
            ),
            tracks: (1...trackCount)
                .map { n in
                    BridgeRawTrackEdit(
                        id: "t-\(n)",
                        title: "Track Title \(n)",
                        artistText: blankTrackArtists
                            ? "" : "Track Artist \(n)",
                        side: 1,
                        trackNumber: Int32(n)
                    )
                }
        )
    }

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
                    localPath: "/tmp/fake/Front.png"
                )
            ],
            documents: [
                BridgeFileInfo(
                    name: "info.log",
                    size: 6000,
                    dirPrefix: nil,
                    fileName: "info.log",
                    localPath: "/tmp/fake/info.log"
                ),
                BridgeFileInfo(
                    name: "notes.txt",
                    size: 1200,
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
            yearMin: 1988,
            yearMax: 1996,
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
                yearMin: 1996,
                yearMax: 1996,
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
                yearMin: 2005,
                yearMax: 2005,
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
