#if DEBUG
    import BaeKit
    import Foundation

    extension PreviewData {
        // MARK: - Config and edit-metadata fixtures

        /// A new preview ConfigStore for each preview scene.
        @MainActor
        static func configStore() -> ConfigStore {
            makeConfigStore(libraryFullWidth: false)
        }

        /// A preview CastStore mid-session, so the casting settings preview shows
        /// the state that asks before the feature is turned off.
        @MainActor
        static func castStore() -> CastStore {
            let store = CastStore()
            store.applyStatus(deviceName: "Living Room Speaker")
            return store
        }

        /// Every metadata source and whether the library asks it, in core's
        /// order. The ordinary library asks both; a preview that is about one
        /// source being off or unreachable names that source's state.
        ///
        /// `canChange` is derived the way core derives it, so a fixture cannot
        /// show a switch as movable that core would refuse to move.
        static func metadataSources(
            musicBrainz: BridgeSourceAvailability = .on,
            discogs: BridgeSourceAvailability = .on
        ) -> [BridgeMetadataSourceSetting] {
            let asked = [musicBrainz, discogs].filter { $0 == .on }
            let onlyAsked = asked.count == 1
            func setting(
                _ source: BridgeMetadataSource,
                _ availability: BridgeSourceAvailability
            ) -> BridgeMetadataSourceSetting {
                BridgeMetadataSourceSetting(
                    source: source,
                    availability: availability,
                    canChange: availability != .notConfigured
                        && !(availability == .on && onlyAsked)
                )
            }
            return [
                setting(.musicBrainz, musicBrainz),
                setting(.discogs, discogs),
            ]
        }

        /// A preview ConfigStore with the given library-width, casting, and
        /// Discogs settings — the previews that vary them build their own;
        /// everything else creates the default through `configStore()` above.
        /// A configured Discogs key is the ordinary library, so the token
        /// stands validated unless a preview asks for the other case.
        ///
        /// The sources follow the token unless a preview names one: a library
        /// with no Discogs token cannot ask Discogs, which is what core would
        /// report, while `discogs` states the case of a reachable source the
        /// person has switched off.
        @MainActor
        static func makeConfigStore(
            libraryFullWidth: Bool,
            castEnabled: Bool = false,
            discogsUsable: Bool = true,
            musicBrainz: BridgeSourceAvailability = .on,
            discogs: BridgeSourceAvailability? = nil
        ) -> ConfigStore {
            ConfigStore(
                config: Config(
                    bridge: BridgeConfig(
                        libraryId: "lib-preview",
                        libraryName: "Preview Library",
                        libraryPath: "/preview",
                        pauseBetweenSides: false,
                        maxConcurrentUploads: 3,
                        maxConcurrentDownloads: 3,
                        identifyAutomatically: true,
                        prefillWithTags: true,
                        metadataSources: metadataSources(
                            musicBrainz: musicBrainz,
                            discogs: discogs
                                ?? (discogsUsable ? .on : .notConfigured)
                        ),
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
                        discogsTokenStatus: discogsUsable
                            ? .valid : .notConfigured,
                        discogsUsable: discogsUsable,
                        sync: nil
                    )
                )
            )
        }

        /// A raw release edit for metadata-editor previews: the album and
        /// pressing fields plus `trackCount` tracks. Blank track artists
        /// exercise the album-artist inheritance path.
        static func editMetadataDraft(
            trackCount: Int,
            blankTrackArtists: Bool = true
        ) -> BridgeRawReleaseEdit {
            BridgeRawReleaseEdit(
                albumTitle: "Album Title",
                albumArtistAssignments: [
                    existingArtist("Artist Name", artistId: "artist-1"),
                    newArtist("New Artist Name"),
                ],
                albumYear: "1983",
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
                            artistAssignments: blankTrackArtists
                                ? .albumArtists
                                : .explicit(
                                    assignments: [
                                        newArtist("Track Artist \(n)")
                                    ]
                                ),
                            side: 1,
                            trackNumber: Int32(n),
                            file: .standalone(fileId: "\(n).flac")
                        )
                    }
            )
        }

        /// A draft credited to a compilation's worth of album artists, most
        /// of them already in the library and two of them new, so the header
        /// has to summarize rather than list.
        static func manyAlbumArtistsDraft() -> BridgeRawReleaseEdit {
            var draft = editMetadataDraft(trackCount: 3)
            draft.albumArtistAssignments =
                (1...10)
                .map { n in
                    existingArtist("Artist Name \(n)", artistId: "artist-\(n)")
                } + [newArtist("New Artist One"), newArtist("New Artist Two")]
            return draft
        }

        static func releaseEditSeed(trackCount: Int) -> BridgeReleaseEditSeed {
            let edit = editMetadataDraft(trackCount: trackCount)
            let format = BridgeAudioFormat(
                codec: "FLAC",
                sampleRateHz: 44_100,
                bitsPerSample: 16,
                bitrateKbps: nil,
                channels: 2
            )
            return BridgeReleaseEditSeed(
                edit: edit,
                canResetToSource: true,
                cover: nil,
                display: BridgeReleaseEditDisplayContext(
                    sourceAudio: .uniform(
                        descriptor: BridgeSourceAudioDescriptor(
                            layout: .file,
                            format: format
                        )
                    ),
                    tracks: edit.tracks.enumerated()
                        .map { index, track in
                            BridgeReleaseEditTrackContext(
                                trackId: track.id,
                                sources: [
                                    BridgeReleaseEditTrackSource(
                                        fileId: "file-\(index + 1)",
                                        name: "Track \(index + 1).flac",
                                        layout: .file
                                    )
                                ],
                                durationMs: Int64(180_000 + index * 12_000),
                                side: .flat,
                                sideHeaderKey: nil
                            )
                        }
                )
            )
        }

        @MainActor
        static func releaseEditor() -> ReleaseEditor {
            ReleaseEditor(
                outboxStore: OutboxStore(
                    snapshot: OutboxStore.emptySnapshot
                ),
                seedReleaseEdit: { _ in
                    releaseEditSeed(trackCount: 5)
                },
                resetReleaseEditToSource: { _ in
                    releaseEditSeed(trackCount: 5).edit
                }
            )
        }

        static func newArtist(_ name: String) -> BridgeArtistAssignment {
            .new(
                seed: BridgeNewArtistSeed(
                    name: name,
                    sortName: nil,
                    musicbrainzArtistId: nil,
                    discogsArtistId: nil
                )
            )
        }

        static func existingArtist(
            _ name: String,
            artistId: String
        ) -> BridgeArtistAssignment {
            .existing(
                artist: BridgeExistingArtist(
                    artistId: artistId,
                    name: name,
                    sortName: nil,
                    musicbrainzArtistId: nil,
                    discogsArtistId: nil
                )
            )
        }

        static func artistAssignmentsLibrary() -> Library {
            Library(searchArtists: { _ in
                [
                    BridgeArtistSearchResult(
                        artist: BridgeExistingArtist(
                            artistId: "artist-1",
                            name: "Artist Name",
                            sortName: "Name, Artist",
                            musicbrainzArtistId: nil,
                            discogsArtistId: nil
                        ),
                        image: nil
                    ),
                    BridgeArtistSearchResult(
                        artist: BridgeExistingArtist(
                            artistId: "artist-2",
                            name: "Artist Name",
                            sortName: "Name, Artist",
                            musicbrainzArtistId: nil,
                            discogsArtistId: nil
                        ),
                        image: nil
                    ),
                ]
            })
        }
    }
#endif
