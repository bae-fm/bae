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

        /// A preview ConfigStore with the given library-width and casting
        /// settings — the previews that vary them build their own; everything
        /// else creates the default through `configStore()` above.
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
                        pauseBetweenSides: false,
                        maxConcurrentUploads: 3,
                        maxConcurrentDownloads: 3,
                        identifyAutomatically: true,
                        defaultImportMetadataSource: .findOnline,
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
