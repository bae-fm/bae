import Foundation

/// Operations that run before a library is open: listing this device's
/// libraries and the restore codes coven saved in iCloud Keychain, deleting
/// a saved code, creating a library, and starting a restore or join. One injectable seam, so shots
/// never read or write real application data.
public final class LibrarySetup: Sendable, Observable {
    public let listLibraries: @Sendable () async throws -> [BridgeLibrary]
    /// How a library stands with its storage: its sync state, when it last
    /// synced and whether it holds unsent changes. An open library answers
    /// from its live sync status; any other is read as it last stood, without
    /// opening it.
    public let libraryStanding:
        @Sendable (_ libraryId: String) async throws ->
            BridgeLibraryStanding
    /// Create a library named by core's default generator, and the profile of
    /// its creator, named by the first argument; returns its open store, its
    /// step written into the setup's stream.
    public let createLibrary:
        @Sendable (_ creatorName: String, BridgeLibrarySetup) async throws ->
            OpenedLibrary
    /// Restore codes coven saved in iCloud Keychain, including ones bae
    /// can't read. Throws when the keychain refuses the lookup — locked, or
    /// the display asleep — which a screen listing them must not read as "no
    /// restore codes".
    public let savedRestoreCodes:
        @Sendable () async throws -> [BridgeSavedRestoreEntry]
    /// Delete the library's saved restore code, readable or not, from iCloud
    /// Keychain on all the person's Apple devices. The library and its
    /// storage stay.
    public let forgetSavedRestoreCode:
        @Sendable (_ libraryId: String) async throws -> Void
    /// The joins this device has under way, to resume.
    public let unfinishedJoins:
        @Sendable () async throws -> [BridgeUnfinishedJoin]
    /// The restores onto this device that stopped after they recorded
    /// themselves, to finish.
    public let unfinishedRestores:
        @Sendable () async throws -> [BridgeUnfinishedRestore]
    /// One restore or join attempt.
    private let makeBootstrap: @MainActor @Sendable () -> LibraryBootstrap
    /// A new setup call's steps, pulled on the process's host.
    private let makeSetupSteps: @Sendable () -> LibrarySetupSteps

    public init(
        listLibraries: @escaping @Sendable () async throws -> [BridgeLibrary],
        libraryStanding:
            @escaping @Sendable (_ libraryId: String) async throws ->
            BridgeLibraryStanding,
        createLibrary:
            @escaping @Sendable (_ creatorName: String, BridgeLibrarySetup)
            async throws -> OpenedLibrary,
        savedRestoreCodes:
            @escaping @Sendable () async throws -> [BridgeSavedRestoreEntry],
        forgetSavedRestoreCode:
            @escaping @Sendable (_ libraryId: String) async throws -> Void,
        unfinishedJoins:
            @escaping @Sendable () async throws -> [BridgeUnfinishedJoin],
        unfinishedRestores:
            @escaping @Sendable () async throws -> [BridgeUnfinishedRestore],
        makeBootstrap: @escaping @MainActor @Sendable () -> LibraryBootstrap,
        makeSetupSteps: @escaping @Sendable () -> LibrarySetupSteps
    ) {
        self.listLibraries = listLibraries
        self.libraryStanding = libraryStanding
        self.createLibrary = createLibrary
        self.savedRestoreCodes = savedRestoreCodes
        self.forgetSavedRestoreCode = forgetSavedRestoreCode
        self.unfinishedJoins = unfinishedJoins
        self.unfinishedRestores = unfinishedRestores
        self.makeBootstrap = makeBootstrap
        self.makeSetupSteps = makeSetupSteps
    }

    /// The steps of a setup call about to start, for its view to draw.
    public func setupSteps() -> LibrarySetupSteps { makeSetupSteps() }

    /// The production wiring over the process's host.
    public static func live(host: BridgeHost, deviceName: String)
        -> LibrarySetup
    {
        LibrarySetup(
            listLibraries: { try await host.listLibraries() },
            libraryStanding: {
                try await host.libraryStanding(libraryId: $0)
            },
            createLibrary: {
                try await host.createLibrary(
                    name: nil,
                    creatorName: $0,
                    setup: $1
                )
            },
            savedRestoreCodes: { try await host.savedRestoreCodes() },
            forgetSavedRestoreCode: {
                try await host.forgetSavedRestoreCode(libraryId: $0)
            },
            unfinishedJoins: { try await host.unfinishedJoins() },
            unfinishedRestores: { try await host.unfinishedRestores() },
            makeBootstrap: {
                LibraryBootstrap(host: host, deviceName: deviceName)
            },
            makeSetupSteps: { LibrarySetupSteps(host: host) }
        )
    }

    /// The state of the step that asks the creator's name and creates the
    /// library, handing it to `onLibraryReady` once made.
    @MainActor
    public func createForm(
        onLibraryReady: @escaping @MainActor (OpenedLibrary) -> Void
    ) -> CreateLibraryForm {
        CreateLibraryForm(
            createLibrary: createLibrary,
            makeSteps: makeSetupSteps,
            onLibraryReady: onLibraryReady
        )
    }

    /// A restore or join screen's state, starting attempts through this seam.
    @MainActor
    public func bootstrapForm(
        entry: BootstrapCodeForm.Entry,
        onLibraryReady:
            @escaping @MainActor (OpenedLibrary, BridgeError?) -> Void
    ) -> BootstrapCodeForm {
        BootstrapCodeForm(
            entry: entry,
            decodeCode: { try decodeCodeInfo(code: $0) },
            makeBootstrap: makeBootstrap,
            unfinishedJoins: unfinishedJoins,
            unfinishedRestores: unfinishedRestores,
            onLibraryReady: onLibraryReady
        )
    }

    #if DEBUG && os(macOS)
        /// Serves `libraries` and `savedCodes`; every other operation throws. A restore, join or delete started from a shot fails at
        /// once.
        public static func stub(
            libraries: [BridgeLibrary] = [],
            savedCodes: [BridgeSavedRestoreEntry] = []
        ) -> LibrarySetup {
            LibrarySetup(
                listLibraries: { libraries },
                libraryStanding: { _ in throw StubError.notImplemented },
                createLibrary: { _, _ in throw StubError.notImplemented },
                savedRestoreCodes: { savedCodes },
                forgetSavedRestoreCode: { _ in
                    throw StubError.notImplemented
                },
                unfinishedJoins: { [] },
                unfinishedRestores: { [] },
                makeBootstrap: {
                    LibraryBootstrap(
                        host: BaeHost.inTemporaryFolder(),
                        deviceName: "Preview Device"
                    )
                },
                makeSetupSteps: {
                    LibrarySetupSteps(host: BaeHost.inTemporaryFolder())
                }
            )
        }
    #endif
}
