import Foundation

/// The writes that make a playlist and change one, as this device's person:
/// make it, add to it, rename it, choose who sees it, take an entry out,
/// move an entry, delete it.
public struct PlaylistWrites: Sendable {
    /// Make a playlist holding `trackIds` in order, and return its id. A
    /// failed write leaves no playlist.
    public let create:
        @Sendable (
            _ name: String, _ sharing: BridgeSharing, _ trackIds: [String]
        ) async throws -> String
    /// Add tracks to the end of a playlist, in the order given. A failed
    /// write adds none.
    public let append:
        @Sendable (_ playlistId: String, _ trackIds: [String]) async throws
            -> Void
    /// Set a playlist's name and who sees it, in one write.
    public let edit:
        @Sendable (
            _ playlistId: String, _ name: String, _ sharing: BridgeSharing
        ) async throws -> Void
    /// Take one entry out of a playlist.
    public let removeEntry:
        @Sendable (_ playlistId: String, _ positionId: String) async throws
            -> Void
    /// Move the entry `positionId` of a playlist to `place`: just before or
    /// just after another entry, or one place up or down.
    public let moveEntry:
        @Sendable (
            _ playlistId: String,
            _ positionId: String,
            _ place: BridgeEntryPlace
        ) async throws -> Void
    /// Delete a playlist, on every device. Its tracks stay in the library.
    public let delete: @Sendable (_ playlistId: String) async throws -> Void

    public init(
        create:
            @escaping @Sendable (String, BridgeSharing, [String]) async throws
            -> String = { _, _, _ in throw StubError.notImplemented },
        append:
            @escaping @Sendable (String, [String]) async throws -> Void = {
                _,
                _ in throw StubError.notImplemented
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
        self.create = create
        self.append = append
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
                create: {
                    try await handle.createPlaylist(
                        name: $0,
                        sharing: $1,
                        trackIds: $2
                    )
                },
                append: {
                    try await handle.appendPlaylistEntries(
                        playlist: $0,
                        trackIds: $1
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
