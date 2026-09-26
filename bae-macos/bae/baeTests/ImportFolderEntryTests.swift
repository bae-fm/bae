import BaeKit
import Foundation
import Testing

@testable import bae

private struct ReadRefused: Error {}

@MainActor
@Suite("Taking in a chosen folder")
struct ImportFolderEntryTests {
    private func folder() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
            .appendingPathComponent("Album")
        try FileManager.default.createDirectory(
            at: url,
            withIntermediateDirectories: true
        )
        return url
    }

    private func take(
        _ url: URL,
        answering chosen: @escaping @Sendable () throws -> BridgeChosenFolder
    ) async -> UiStore {
        let uiStore = UiStore()
        let entry = ImportFolderEntry(
            importer: Importer(chooseFolder: { _ in try chosen() }),
            uiStore: uiStore
        )
        let task = entry.take(url)
        #expect(uiStore.foldersBeingRead.map(\.name) == [url.lastPathComponent])
        await task?.value
        #expect(uiStore.foldersBeingRead.isEmpty)
        return uiStore
    }

    @Test("a folder the library already holds shows its album")
    func inLibraryShowsTheAlbum() async throws {
        let uiStore = await take(try folder()) {
            .inLibrary(albumId: "album-1")
        }

        #expect(uiStore.activeSection == .library)
        #expect(uiStore.selectedAlbumId == "album-1")
        #expect(uiStore.pendingAlbumReveal?.albumId == "album-1")
    }

    @Test("a folder with releases still to import selects and reveals them")
    func waitingReleasesAreSelectedAndRevealed() async throws {
        let uiStore = await take(try folder()) {
            .inImportQueue(candidateKeys: ["/music/Album 1", "/music/Album 2"])
        }

        #expect(uiStore.activeSection == .importing)
        #expect(
            uiStore.selectedFolderCandidates == [
                "/music/Album 1", "/music/Album 2",
            ]
        )
        #expect(
            uiStore.pendingImportCandidateReveal?.candidateKey
                == "/music/Album 1"
        )
    }

    @Test("a folder with no releases goes to the import tab")
    func noReleasesGoesToImport() async throws {
        let uiStore = await take(try folder()) { .noReleases }

        #expect(uiStore.activeSection == .importing)
        #expect(uiStore.pendingImportCandidateReveal == nil)
    }

    @Test("a failed read is reported and goes nowhere")
    func failureIsReported() async throws {
        let uiStore = await take(try folder()) { throw ReadRefused() }

        #expect(uiStore.activeSection == .library)
        #expect(uiStore.lastError != nil)
    }

    @Test("a file is refused before anything is read")
    func aFileIsRefused() throws {
        let file = try folder().appendingPathComponent("01 Track.flac")
        FileManager.default.createFile(atPath: file.path, contents: Data())
        let uiStore = UiStore()
        let entry = ImportFolderEntry(
            importer: Importer(chooseFolder: { _ in
                Issue.record("a file is never read")
                return .noReleases
            }),
            uiStore: uiStore
        )

        #expect(entry.take(file) == nil)
        #expect(uiStore.lastError != nil)
        #expect(uiStore.foldersBeingRead.isEmpty)
    }
}
