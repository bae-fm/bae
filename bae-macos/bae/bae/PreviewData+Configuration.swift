#if DEBUG
    import BaeKit
    import Foundation

    extension PreviewData {
        /// A new shot ConfigStore for each shot scene.
        @MainActor
        static func configStore(
            storage: BridgeStorageLocation? = nil
        ) -> ConfigStore {
            makeConfigStore(libraryFullWidth: false, storage: storage)
        }

        /// Every metadata source and whether the library asks it, in core's
        /// order. The ordinary library asks both; a shot that is about one
        /// source being off or unreachable names that source's state.
        ///
        /// `canChange` is derived the way core derives it, so a fixture cannot
        /// show a switch as movable that core would refuse to move.
        static func lookupCatalogs(
            musicBrainz: BridgeSourceAvailability = .on,
            discogs: BridgeSourceAvailability = .on
        ) -> [BridgeLookupCatalogSetting] {
            let asked = [musicBrainz, discogs].filter { $0 == .on }
            let onlyAsked = asked.count == 1
            func setting(
                _ catalog: BridgeLookupCatalog,
                _ availability: BridgeSourceAvailability
            ) -> BridgeLookupCatalogSetting {
                BridgeLookupCatalogSetting(
                    catalog: catalog,
                    availability: availability,
                    canChange: !(availability == .on && onlyAsked)
                )
            }
            return [
                setting(.musicBrainz, musicBrainz),
                setting(.discogs, discogs),
            ]
        }

        /// A shot ConfigStore with the given library-width, side-pause,
        /// casting, MCP server, identification, and Discogs settings — the
        /// shots that vary them build their own;
        /// everything else creates the default through `configStore()` above.
        /// A configured Discogs key is the ordinary library, so the token
        /// stands validated unless a shot asks for the other case.
        ///
        /// The sources follow the token unless a shot names one: a library
        /// with no Discogs token cannot ask Discogs, which is what core would
        /// report, while `discogs` states the case of a reachable source the
        /// person has switched off.
        @MainActor
        static func makeConfigStore(
            libraryFullWidth: Bool,
            pauseBetweenSides: Bool = false,
            castEnabled: Bool = false,
            mcpEnabled: Bool = false,
            identifyAutomatically: Bool = true,
            discogsUsable: Bool = true,
            musicBrainz: BridgeSourceAvailability = .on,
            discogs: BridgeSourceAvailability? = nil,
            discogsTokenStatus: BridgeDiscogsTokenStatus? = nil,
            importWhenIdentified: Bool = false,
            importDestination: BridgeImportDestination = .cloud,
            storage: BridgeStorageLocation? = nil,
            savePresets: [BridgeSavePreset] = PreviewData.savePresets,
            lastSavePreset: String = "flac",
            filenamePieces: [BridgeSaveFilenamePiece] = PreviewData
                .exportFilenamePieces,
            filenamePatternError: BridgeNamePatternError? = nil,
            folderPieces: [BridgeSaveFolderPiece] = PreviewData
                .exportFolderPieces,
            folderPatternError: BridgeNamePatternError? = nil
        ) -> ConfigStore {
            let tokenStatus =
                discogsTokenStatus ?? (discogsUsable ? .valid : .notConfigured)
            let catalogs = lookupCatalogs(
                musicBrainz: musicBrainz,
                discogs: discogs ?? (discogsUsable ? .on : .notConfigured)
            )
            return ConfigStore(
                config: Config(
                    bridge: BridgeConfig(
                        libraryId: "lib-preview",
                        libraryPath: "/preview",
                        pauseBetweenSides: pauseBetweenSides,
                        sidePauseCountdown: .off,
                        maxConcurrentUploads: 3,
                        maxConcurrentDownloads: 3,
                        maxConcurrentTransfers: 8,
                        identifyAutomatically: identifyAutomatically,
                        importWhenIdentified: importWhenIdentified,
                        importDestination: importDestination,
                        lookupCatalogs: catalogs,
                        // As core answers it: whether any catalog is asked.
                        hasSourceToSearch: catalogs.contains {
                            $0.availability == .on
                        },
                        showRemainingTime: false,
                        libraryFullWidth: libraryFullWidth,
                        askBeforeUnpinning: true,
                        savePresets: savePresets,
                        filenamePieces: filenamePieces,
                        filenamePatternError: filenamePatternError,
                        folderPieces: folderPieces,
                        folderPatternError: folderPatternError,
                        saveExtras: PreviewData.saveExtras,
                        lastSavePreset: lastSavePreset,
                        castEnabled: castEnabled,
                        castTranscodeFormat: .mp3,
                        mcp: BridgeMcpConfig(enabled: mcpEnabled, port: 47777),
                        subsonic: BridgeSubsonicConfig(
                            enabled: false,
                            port: 4533,
                            username: "",
                            network: .thisMachine
                        ),
                        discogsTokenStatus: tokenStatus,
                        discogsUsable: tokenStatus == .valid
                            || tokenStatus == .unvalidated
                    )
                ),
                storage: storage
            )
        }

    }
#endif
