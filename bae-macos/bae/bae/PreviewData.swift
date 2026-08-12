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
        /// for core's queue subscription. Echo-driven view behavior (the
        /// post-reorder hold clearing, the removal unmount, the shuffle flip)
        /// only works in a preview against this; against `Queue.stub()` the echo
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

    }
#endif
