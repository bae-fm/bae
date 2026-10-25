#if BAE_DEV_RESET
    import BaeKit
    import Foundation
    import os.log

    private let logger = Logger.bae("DevReset")

    /// Why a dev reset stopped. The debug app shows it on the welcome screen.
    enum DevResetError: LocalizedError {
        /// The library left its store, but this Mac's files or keys for it
        /// remain.
        case removalUnfinished(libraryId: String, BridgeError)
        case discogsTokenRejected

        var errorDescription: String? {
            switch self {
            case .removalUnfinished(let libraryId, let error):
                "Dev reset: removing library \(libraryId) didn't finish: \(String(reflecting: error))"
            case .discogsTokenRejected:
                "Dev reset: Discogs rejected the profile's token"
            }
        }
    }

    /// Wipes this Mac's bae state and recreates one library from a dev
    /// profile, before any library opens. Every library is removed from this
    /// Mac without opening it or reading its store: its directory and its
    /// store keys in the keychain go, and its saved restore code is deleted the
    /// way the welcome screen deletes one. Then `~/.bae` goes, and the new
    /// library is created and set up the way Settings and storage setup do.
    /// `scripts/reset-bae.py` empties the profile's storage prefix before it
    /// launches the app, so the new library starts on an empty remote. Each
    /// step logs; the first failure stops the reset and is thrown.
    struct DevReset: Sendable {
        let readProfile: @Sendable (String) throws -> BridgeDevResetProfile
        /// Removals an earlier run left unfinished.
        let finishRemovals: @Sendable () async throws -> Void
        let listLibraries: @Sendable () async throws -> [BridgeLibrary]
        /// Remove the library from this Mac only. The store isn't told this
        /// device left: the reset script emptied the storage prefix before
        /// the app launched.
        let removeLibrary:
            @Sendable (BridgeLibrary) async throws -> BridgeLibraryRemoval
        let savedRestoreCodes:
            @Sendable () async throws -> [BridgeSavedRestoreEntry]
        let forgetSavedRestoreCode: @Sendable (String) async throws -> Void
        /// Delete bae's directory and create it empty again: the host's app
        /// directory must exist while it runs.
        let replaceAppDirectory: @Sendable () throws -> Void
        /// Create the profile's library, set it up, close it, and return its
        /// id.
        let createLibrary:
            @Sendable (BridgeDevResetProfile) async throws -> String

        /// Run the reset; returns the new library's id, for the app to open.
        func run(profilePath: String) async throws -> String {
            logger.info("reading profile \(profilePath)")
            let profile = try readProfile(profilePath)

            logger.info("finishing earlier unfinished removals")
            try await finishRemovals()

            let libraries = try await listLibraries()
            logger.info("removing \(libraries.count) libraries")
            for library in libraries {
                logger.info("removing library \(library.id)")
                if case .unfinished(let removal) = try await removeLibrary(
                    library
                ) {
                    throw DevResetError.removalUnfinished(
                        libraryId: library.id,
                        removal.error
                    )
                }
            }

            let removed = Set(libraries.map(\.id))
            for entry in try await savedRestoreCodes() {
                let libraryId = entry.libraryId
                guard removed.contains(libraryId) else {
                    logger.info(
                        "keeping the saved restore code of \(libraryId): it isn't a library on this Mac"
                    )
                    continue
                }
                logger.info("deleting the saved restore code of \(libraryId)")
                try await forgetSavedRestoreCode(libraryId)
            }

            logger.info("replacing ~/.bae")
            try replaceAppDirectory()

            logger.info("creating library \(profile.libraryName)")
            let libraryId = try await createLibrary(profile)
            logger.info("created library \(libraryId)")
            return libraryId
        }
    }

    extension BridgeSavedRestoreEntry {
        fileprivate var libraryId: String {
            switch self {
            case .readable(let code): code.info().libraryId
            case .unreadable(let libraryId, _): libraryId
            }
        }
    }

    extension DevReset {
        /// The production wiring over the process's host.
        static func live(
            host: BridgeHost,
            home: String,
            deviceName: String
        ) -> DevReset {
            DevReset(
                readProfile: { try readDevResetProfile(path: $0) },
                finishRemovals: { try await host.finishRemovals() },
                listLibraries: { try await host.listLibraries() },
                removeLibrary: {
                    try await host.removeUnopenableLibrary(libraryId: $0.id)
                },
                savedRestoreCodes: { try await host.savedRestoreCodes() },
                forgetSavedRestoreCode: {
                    try await host.forgetSavedRestoreCode(libraryId: $0)
                },
                replaceAppDirectory: {
                    let directory = URL(fileURLWithPath: home)
                        .appendingPathComponent(".bae", isDirectory: true)
                    try FileManager.default.removeItem(at: directory)
                    try FileManager.default.createDirectory(
                        at: directory,
                        withIntermediateDirectories: false
                    )
                },
                createLibrary: { profile in
                    try await createAndSetUp(
                        profile,
                        host: host,
                        deviceName: deviceName
                    )
                }
            )
        }

        /// Create the library and set it up as Settings and storage setup
        /// would: the Discogs token, the two metadata preferences, then the S3
        /// storage. The handle closes before the app opens the library again.
        private static func createAndSetUp(
            _ profile: BridgeDevResetProfile,
            host: BridgeHost,
            deviceName: String
        ) async throws -> String {
            let created = try await host.createLibrary(
                name: profile.libraryName
            )
            let libraryId = created.libraryId()
            let app = try await DetachedWork.run {
                try created.open(
                    positionUpdateIntervalMs: 200,
                    restorePlayback: false
                )
            }
            do {
                logger.info("saving the Discogs token")
                if try await app.saveDiscogsToken(token: profile.discogsToken)
                    == .rejected
                {
                    throw DevResetError.discogsTokenRejected
                }
                logger.info(
                    "identify automatically: \(profile.identifyAutomatically)"
                )
                try await app.setIdentifyAutomatically(
                    enabled: profile.identifyAutomatically
                )
                logger.info(
                    "import when identified: \(profile.importWhenIdentified)"
                )
                try await app.setImportWhenIdentified(
                    enabled: profile.importWhenIdentified
                )
                logger.info("connecting S3 storage")
                _ = try await app.saveSyncConfig(
                    configData: profile.syncConfig,
                    deviceName: deviceName
                )
            }
            catch {
                do {
                    try await DetachedWork.run { try app.closeLibrary() }
                }
                catch let closeError {
                    logger.error(
                        "couldn't close \(libraryId) after its setup stopped: \(String(reflecting: closeError))"
                    )
                }
                throw error
            }
            try await DetachedWork.run { try app.closeLibrary() }
            return libraryId
        }
    }
#endif
