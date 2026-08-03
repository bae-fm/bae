#if DEBUG
    import BaeKit
    import Foundation

    /// The in-memory queue behind `PreviewData.echoingQueue`: holds the two
    /// lanes, applies each command the way core would, and pushes the result
    /// into the store as a fresh snapshot. A dependency fake — the views under
    /// preview run their production code against it.
    @MainActor
    private final class PreviewQueueModel {
        private let store: PlaybackStore
        private var manual: [BridgeQueueEntry]
        private var upcoming: [BridgeQueueEntry]
        /// The context's unshuffled order, kept so turning shuffle off restores
        /// it (filtered to entries still present).
        private let orderedUpcoming: [BridgeQueueEntry]
        private var hasContext: Bool
        private var shuffled: Bool
        private var revision: UInt64 = 1
        /// Uniques the entry ids this model mints for inserted tracks.
        private var mintedEntryCount = 0

        init(
            store: PlaybackStore,
            manual: [BridgeQueueEntry],
            upcoming: [BridgeQueueEntry],
            hasContext: Bool,
            shuffled: Bool
        ) {
            self.store = store
            self.manual = manual
            self.upcoming = upcoming
            orderedUpcoming = upcoming
            self.hasContext = hasContext
            self.shuffled = shuffled
            apply()
        }

        private func apply() {
            revision += 1
            store.applyQueueSnapshot(
                BridgeQueueSnapshot(
                    manual: manual,
                    context: hasContext
                        ? BridgePlaybackContext(
                            kind: .release,
                            sourceTitle: "Neon Frequencies",
                            shuffled: shuffled,
                            upcoming: upcoming,
                            upcomingTotal: UInt64(upcoming.count)
                        ) : nil,
                    hasNext: !manual.isEmpty || !upcoming.isEmpty,
                    hasPrevious: false,
                    revision: revision
                )
            )
        }

        func remove(_ entryId: String) {
            manual.removeAll { $0.entryId == entryId }
            upcoming.removeAll { $0.entryId == entryId }
            apply()
        }

        func clearUpNext() {
            manual = []
            apply()
        }

        /// Drop the context lane: its rows and its section go, the now-playing
        /// track stays — core's `clear_playing_from`.
        func clearPlayingFrom() {
            upcoming = []
            hasContext = false
            apply()
        }

        /// Move `entryId` before `beforeEntryId` within its lane; `nil` moves
        /// it to the lane's end — core's reorder semantics.
        func reorder(_ entryId: String, before beforeEntryId: String?) {
            func moved(in lane: [BridgeQueueEntry]) -> [BridgeQueueEntry]? {
                guard
                    let from = lane.firstIndex(where: {
                        $0.entryId == entryId
                    })
                else {
                    return nil
                }
                var lane = lane
                let entry = lane.remove(at: from)
                if let beforeEntryId,
                    let to = lane.firstIndex(where: {
                        $0.entryId == beforeEntryId
                    })
                {
                    lane.insert(entry, at: to)
                }
                else {
                    lane.append(entry)
                }
                return lane
            }
            if let lane = moved(in: manual) {
                manual = lane
            }
            else if let lane = moved(in: upcoming) {
                upcoming = lane
            }
            apply()
        }

        /// Play the entry: it becomes now playing, and it — plus whatever sat
        /// before it in its lane — leaves the queue, the way core drains
        /// skipped-over entries.
        func skipTo(_ entryId: String) {
            let entry: BridgeQueueEntry
            if let index = manual.firstIndex(where: { $0.entryId == entryId }) {
                entry = manual[index]
                manual.removeSubrange(...index)
            }
            else if let index = upcoming.firstIndex(where: {
                $0.entryId == entryId
            }) {
                entry = upcoming[index]
                upcoming.removeSubrange(...index)
            }
            else {
                return
            }
            store.play(
                track: NowPlayingTrack(
                    trackId: entry.trackId,
                    trackTitle: entry.title,
                    artistNames: entry.artistNames,
                    albumId: "a-01",
                    coverImage: entry.coverImage,
                    // The queue entry carries only a clock label, not raw ms;
                    // the preview now-playing bar just needs a plausible total.
                    durationMs: 200_000
                )
            )
            apply()
        }

        func setShuffle(_ on: Bool) {
            shuffled = on
            if on {
                upcoming.shuffle()
            }
            else {
                let present = Set(upcoming.map(\.entryId))
                upcoming = orderedUpcoming.filter {
                    present.contains($0.entryId)
                }
            }
            apply()
        }

        func insert(trackIds: [String], at index: Int) {
            // A cross-lane drag inserts a context row's track: carry its
            // metadata over. Unknown ids (a library drop) get a bare entry.
            let entries = trackIds.map { trackId in
                mintedEntryCount += 1
                let source =
                    upcoming.first { $0.trackId == trackId }
                    ?? PreviewData.queueEntries.first {
                        $0.trackId == trackId
                    }
                return BridgeQueueEntry(
                    entryId: "preview-minted-\(mintedEntryCount)",
                    trackId: trackId,
                    title: source?.title ?? "Track \(trackId)",
                    artistNames: source?.artistNames ?? "Artist Name",
                    durationClock: source?.durationClock
                        ?? bridgeClock(ms: 200_000),
                    albumTitle: source?.albumTitle ?? "Album Title",
                    coverImage: source?.coverImage
                )
            }
            let at = min(max(index, 0), manual.count)
            manual.insert(contentsOf: entries, at: at)
            apply()
        }
    }

    /// Shared sample data for SwiftUI #Preview blocks.
    /// Album/artist/track names match bae-mocks/fixtures/data.json.
    enum PreviewData {
        // MARK: - Albums

        /// The grid list, derived from the detail payloads so the grid summaries and
        /// the seeded `releaseDetails` agree on release ids. Ordered a-01..a-22,
        /// matching `albumDetailList`.
        static let albums: [BridgeAlbum] = albumDetailList.map(\.album)

        // MARK: - Queue

        static let queueEntries: [BridgeQueueEntry] = [
            BridgeQueueEntry(
                entryId: "e-01",
                trackId: "t-01",
                title: "Track Title 1",
                artistNames: "Artist Name A",
                durationClock: bridgeClock(ms: 210_000),
                albumTitle: "Album Title A",
                coverImage: nil
            ),
            BridgeQueueEntry(
                entryId: "e-02",
                trackId: "t-02",
                title: "Track Title 2",
                artistNames: "Artist Name A",
                durationClock: bridgeClock(ms: 240_000),
                albumTitle: "Album Title A",
                coverImage: nil
            ),
            BridgeQueueEntry(
                entryId: "e-03",
                trackId: "t-03",
                title: "Track Title 3",
                artistNames: "Artist Name B",
                durationClock: bridgeClock(ms: 198_000),
                albumTitle: "Album Title B",
                coverImage: nil
            ),
            BridgeQueueEntry(
                entryId: "e-04",
                trackId: "t-04",
                title: "Track Title 4",
                artistNames: "Artist Name B",
                durationClock: bridgeClock(ms: 225_000),
                albumTitle: "Album Title B",
                coverImage: nil
            ),
            BridgeQueueEntry(
                entryId: "e-05",
                trackId: "t-05",
                title: "Track Title 5",
                artistNames: "Artist Name C",
                durationClock: bridgeClock(ms: 187_000),
                albumTitle: "Album Title C",
                coverImage: nil
            ),
        ]

        /// A `PlaybackStore` preloaded with `queueEntries`: `manualCount` in the
        /// manual lane, the rest in the context tail. Used by previews that need a
        /// live store in the environment (`QueueView` reads its queue state via
        /// `@Environment`, not by-value props).
        @MainActor
        static func queueStore(
            manualCount: Int,
            context: BridgePlaybackSourceKind? = .release,
            shuffled: Bool = false
        ) -> PlaybackStore {
            let store = PlaybackStore()
            let manual = Array(queueEntries.prefix(manualCount))
            let upcoming = Array(queueEntries.suffix(from: manualCount))
            store.applyQueueSnapshot(
                BridgeQueueSnapshot(
                    manual: manual,
                    context: context.map { kind in
                        BridgePlaybackContext(
                            kind: kind,
                            sourceTitle: kind == .release
                                ? "Neon Frequencies" : nil,
                            shuffled: shuffled,
                            upcoming: upcoming,
                            upcomingTotal: UInt64(upcoming.count)
                        )
                    },
                    hasNext: !upcoming.isEmpty,
                    hasPrevious: false,
                    revision: 1
                )
            )
            return store
        }

        /// A store plus a `Queue` whose commands mutate an in-memory queue and
        /// re-apply a fresh snapshot (revision bumped) — the preview stand-in
        /// for core's `QueueUpdated` echo. Echo-driven view behavior (the
        /// post-reorder hold clearing, the removal unmount, the shuffle flip)
        /// only works in a preview against this; against `Queue.stub` the echo
        /// never lands and a second drag starts from a stale display order.
        @MainActor
        static func echoingQueue(
            manualCount: Int,
            context: Bool = true,
            shuffled: Bool = false
        ) -> (store: PlaybackStore, queue: Queue) {
            let store = PlaybackStore()
            let model = PreviewQueueModel(
                store: store,
                manual: Array(queueEntries.prefix(manualCount)),
                upcoming: context
                    ? Array(queueEntries.suffix(from: manualCount)) : [],
                hasContext: context,
                shuffled: shuffled
            )
            return (
                store,
                Queue(
                    insertInQueue: { ids, index in
                        Task { @MainActor in
                            model.insert(trackIds: ids, at: Int(index))
                        }
                    },
                    removeEntry: { id in
                        Task { @MainActor in
                            model.remove(id)
                        }
                    },
                    clearUpNext: {
                        Task { @MainActor in
                            model.clearUpNext()
                        }
                    },
                    clearPlayingFrom: {
                        Task { @MainActor in
                            model.clearPlayingFrom()
                        }
                    },
                    reorderEntry: { id, before in
                        Task { @MainActor in
                            model.reorder(id, before: before)
                        }
                    },
                    skipToEntry: { id in
                        Task { @MainActor in
                            model.skipTo(id)
                        }
                    },
                    setShuffle: { on in
                        Task { @MainActor in
                            model.setShuffle(on)
                        }
                    }
                )
            )
        }

        // MARK: - Now Playing

        static let nowPlayingTitle = "Broadcast"
        static let nowPlayingArtist = "The Midnight Signal"

        // MARK: - Search

        /// Search results for the SearchView "With results" preview.
        static let searchResults = SearchResults(
            bridge: BridgeSearchResults(
                albums: [
                    BridgeAlbumSearchResult(
                        id: "a-02",
                        title: "Album Title B",
                        year: 2019,
                        artistName: "Artist Name A",
                        cover: nil
                    ),
                    BridgeAlbumSearchResult(
                        id: "a-14",
                        title: "Album Title N",
                        year: 2022,
                        artistName: "Artist Name A",
                        cover: nil
                    ),
                    BridgeAlbumSearchResult(
                        id: "a-03",
                        title: "Album Title C",
                        year: 2021,
                        artistName: "Artist Name B",
                        cover: nil
                    ),
                ],
                artists: [
                    BridgeArtistSummary(
                        artistId: "artist-a",
                        name: "Artist Name A",
                        albumCount: 2,
                        image: nil
                    )
                ],
                tracks: [
                    BridgeTrackSearchResult(
                        id: "t-03",
                        title: "Track Title 3",
                        durationClock: bridgeClock(ms: 198_000),
                        albumId: "a-02",
                        albumTitle: "Album Title B",
                        artistName: "Artist Name A",
                        cover: nil
                    ),
                    BridgeTrackSearchResult(
                        id: "t-05",
                        title: "Track Title 5",
                        durationClock: bridgeClock(ms: 187_000),
                        albumId: "a-03",
                        albumTitle: "Album Title C",
                        artistName: "Artist Name B",
                        cover: nil
                    ),
                ],
                composers: [
                    BridgeComposerSummary(
                        artistId: "artist-composer-a",
                        name: "Composer Name A",
                        sortName: "Composer Name A",
                        workCount: 2,
                        linkedReleaseCount: 3,
                        unlinkedCreditCount: 0,
                        image: nil
                    )
                ],
                works: [
                    BridgeWorkSummary(
                        workId: "work-a",
                        title: "Work Title A",
                        disambiguation: nil,
                        workType: "work",
                        parentWorkId: nil,
                        composerNames: "Composer Name A",
                        linkedReleaseCount: 2,
                        representativeReleaseId: "release-a",
                        representativeCover: nil
                    )
                ],
            ),
            query: "placeholder"
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
                    isVinyl
                        ? "\(letter)\(index + 1)" : "\(sideNumber)-\(index + 1)"
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
                        tracks: tracks
                    )
                )
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
                    cover: nil,
                ),
                releases: [
                    BridgeRelease(
                        id: "rel-\(id)",
                        albumId: id,
                        displayName: "\(year) \(format)",
                        year: year,
                        format: format,
                        label: nil,
                        catalogNumber: nil,
                        country: nil,
                        storageState: .local,
                        pinned: false,
                        storageActions: [],
                        transferAction: nil,
                        tracks: makeTracks(tracks, artist: artist),
                        trackGroups: [
                            BridgeTrackGroup(
                                side: .flat,
                                headerKey: nil,
                                tracks: makeTracks(tracks, artist: artist)
                            )
                        ],
                        files: [],
                        imageFiles: [],
                        galleryItems: [],
                        totalDuration: .minutesOnly(minutes: 39),
                        fileCount: 0,
                        totalSize: 0,
                        cover: nil,
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
            let isVinyl =
                format.contains("Vinyl") || format.contains("Cassette")
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
                    cover: nil,
                ),
                releases: [
                    BridgeRelease(
                        id: "rel-\(id)",
                        albumId: id,
                        displayName: "\(year) \(format)",
                        year: year,
                        format: format,
                        label: "Some Label",
                        catalogNumber: "CAT-001",
                        country: "US",
                        storageState: .local,
                        pinned: false,
                        storageActions: [],
                        transferAction: nil,
                        tracks: sides.tracks,
                        trackGroups: sides.groups,
                        files: [],
                        imageFiles: [],
                        galleryItems: [],
                        totalDuration: .minutesOnly(minutes: 39),
                        fileCount: 0,
                        totalSize: 0,
                        cover: nil,
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
                releaseName: String?, year: Int32, format: String,
                tracks: [String],
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
                                headerKey: nil,
                                tracks: allTracks
                            )
                        ]
                    }
                    return BridgeRelease(
                        id: releaseIds[index],
                        albumId: id,
                        displayName: displayName,
                        year: rel.year,
                        format: rel.format,
                        label: nil,
                        catalogNumber: nil,
                        country: nil,
                        storageState: .local,
                        pinned: false,
                        storageActions: [],
                        transferAction: nil,
                        tracks: allTracks,
                        trackGroups: groups,
                        files: [],
                        imageFiles: [],
                        galleryItems: [],
                        totalDuration: .minutesOnly(minutes: 39),
                        fileCount: 0,
                        totalSize: 0,
                        cover: nil,
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
                    cover: nil,
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

        // MARK: - Config and edit-metadata fixtures

        /// Shared preview ConfigStore. ConfigStore is a non-Sendable `@Observable`,
        /// so it needs `@MainActor` isolation to hold as a static. The explicit
        /// type keeps the static preview analyzer able to see that
        /// `.environment(PreviewData.configStore)` provides a `ConfigStore`.
        @MainActor
        static let configStore: ConfigStore = makeConfigStore(
            libraryFullWidth: false
        )

        /// A preview CastStore mid-session, so the casting settings preview shows
        /// the state that asks before the feature is turned off.
        @MainActor
        static let castStore: CastStore = {
            let store = CastStore()
            store.applyStatus(deviceName: "Living Room Speaker")
            return store
        }()

        /// A preview ConfigStore with the given library-width and casting
        /// settings — the previews that vary them build their own; everything
        /// else shares the `configStore` static above.
        @MainActor
        static func makeConfigStore(
            libraryFullWidth: Bool,
            castEnabled: Bool = false
        ) -> ConfigStore {
            ConfigStore(
                config: Config(
                    bridge: BridgeConfig(
                        libraryId: "lib-preview",
                        libraryName: "Preview Library",
                        libraryPath: "/preview",
                        encryptionKeyStored: false,
                        encryptionKeyFingerprint: nil,
                        pauseBetweenSides: false,
                        maxConcurrentUploads: 3,
                        maxConcurrentDownloads: 3,
                        showRemainingTime: false,
                        libraryFullWidth: libraryFullWidth,
                        savePresets: savePresets,
                        defaultTrackSavePreset: "flac",
                        defaultReleaseSavePreset: "flac",
                        castEnabled: castEnabled,
                        mcp: BridgeMcpConfig(enabled: false, port: 47777),
                        subsonic: BridgeSubsonicConfig(
                            enabled: false,
                            port: 4533,
                            username: "",
                            bindAddress: "127.0.0.1"
                        ),
                        discogsTokenStatus: .notConfigured,
                        discogsUsable: false,
                        sync: nil
                    )
                ),
                syncReady: false
            )
        }

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
                            trackNumber: Int32(n),
                            file: .standalone(fileId: "\(n).flac")
                        )
                    }
            )
        }
    }
#endif
