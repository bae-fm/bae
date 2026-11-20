import Foundation

/// The writes that make a playlist and change one, as this device's person:
/// make it, add to it at its end or at a place, rename it, choose who sees it, take an entry out,
/// move an entry, delete it.
public struct PlaylistWrites: Sendable {
    /// Make a playlist holding the tracks a person picked, `trackIds` in no
    /// order, in the order their release lists them, and return its id. The
    /// playlist holds the picks the library holds. A failed write leaves no
    /// playlist.
    public let createOfPicked:
        @Sendable (
            _ name: String, _ sharing: BridgeSharing, _ trackIds: [String]
        ) async throws -> String
    /// Add the tracks a person picked, `trackIds` in no order, to the end of
    /// a playlist, in the order their release lists them. A failed write,
    /// such as one naming a track the library lacks, adds none.
    public let appendPicked:
        @Sendable (_ playlistId: String, _ trackIds: [String]) async throws
            -> Void
    /// Add the tracks a person picked, `trackIds` in no order, at `place` in
    /// a playlist, in the order their release lists them. A failed write,
    /// such as one naming a track the library lacks or an entry the playlist
    /// no longer holds, adds none.
    public let insertPicked:
        @Sendable (
            _ playlistId: String,
            _ trackIds: [String],
            _ place: BridgeBesideEntry
        ) async throws -> Void
    /// Set a playlist's name and who sees it, in one write.
    public let edit:
        @Sendable (
            _ playlistId: String, _ name: String, _ sharing: BridgeSharing
        ) async throws -> Void
    /// Take one entry out of a playlist.
    public let removeEntry:
        @Sendable (_ playlistId: String, _ positionId: String) async throws
            -> Void
    /// Move the entry `positionId` of a playlist to `place`: beside another
    /// entry, or one place up or down.
    public let moveEntry:
        @Sendable (
            _ playlistId: String,
            _ positionId: String,
            _ place: BridgeEntryPlace
        ) async throws -> Void
    /// Delete a playlist, on every device. Its tracks stay in the library.
    public let delete: @Sendable (_ playlistId: String) async throws -> Void

    public init(
        createOfPicked:
            @escaping @Sendable (String, BridgeSharing, [String]) async throws
            -> String = { _, _, _ in throw StubError.notImplemented },
        appendPicked:
            @escaping @Sendable (String, [String]) async throws -> Void = {
                _,
                _ in throw StubError.notImplemented
            },
        insertPicked:
            @escaping @Sendable (String, [String], BridgeBesideEntry)
            async throws -> Void = { _, _, _ in
                throw StubError.notImplemented
            },
        edit:
            @escaping @Sendable (String, String, BridgeSharing) async throws
            -> Void = { _, _, _ in throw StubError.notImplemented },
        removeEntry:
            @escaping @Sendable (String, String) async throws -> Void = {
                _,
                _ in throw StubError.notImplemented
            },
        moveEntry:
            @escaping @Sendable (String, String, BridgeEntryPlace) async throws
            -> Void =
            {
                _,
                _,
                _ in throw StubError.notImplemented
            },
        delete: @escaping @Sendable (String) async throws -> Void = {
            _ in throw StubError.notImplemented
        }
    ) {
        self.createOfPicked = createOfPicked
        self.appendPicked = appendPicked
        self.insertPicked = insertPicked
        self.edit = edit
        self.removeEntry = removeEntry
        self.moveEntry = moveEntry
        self.delete = delete
    }
}

#if os(macOS)
    extension PlaylistWrites {
        /// The writes through `handle`.
        static func live(_ handle: any AppHandleProtocol) -> PlaylistWrites {
            PlaylistWrites(
                createOfPicked: {
                    try await handle.createPlaylistOfPicked(
                        name: $0,
                        sharing: $1,
                        trackIds: $2
                    )
                },
                appendPicked: {
                    try await handle.appendPickedToPlaylist(
                        playlist: $0,
                        trackIds: $1
                    )
                },
                insertPicked: {
                    try await handle.insertPickedIntoPlaylist(
                        playlist: $0,
                        trackIds: $1,
                        place: $2
                    )
                },
                edit: {
                    try await handle.editPlaylist(
                        id: $0,
                        name: $1,
                        sharing: $2
                    )
                },
                removeEntry: {
                    try await handle.removePlaylistEntry(
                        playlist: $0,
                        position: $1
                    )
                },
                moveEntry: {
                    try await handle.movePlaylistEntry(
                        playlist: $0,
                        position: $1,
                        place: $2
                    )
                },
                delete: { try await handle.deletePlaylist(id: $0) }
            )
        }
    }
#endif
