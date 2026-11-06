import BaeKit
import Foundation
import Testing
import UniformTypeIdentifiers

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

    @Test("a chosen folder goes to the import tab before core answers")
    func aFolderGoesToImportAtOnce() async throws {
        let uiStore = UiStore()
        let entry = ImportFolderEntry(
            importer: Importer(addWatchedFolder: { _ in }),
            uiStore: uiStore
        )

        let task = entry.take(try folder())
        #expect(uiStore.activeSection == .importing)
        await task?.value
        #expect(uiStore.activeSection == .importing)
        #expect(uiStore.lastError == nil)
    }

    @Test("a failed add is reported")
    func failureIsReported() async throws {
        let uiStore = UiStore()
        let entry = ImportFolderEntry(
            importer: Importer(addWatchedFolder: { _ in throw ReadRefused() }),
            uiStore: uiStore
        )

        await entry.take(try folder())?.value

        #expect(uiStore.lastError != nil)
    }

    @Test("a file is refused before anything is added")
    func aFileIsRefused() throws {
        let file = try folder().appendingPathComponent("01 Track.flac")
        FileManager.default.createFile(atPath: file.path, contents: Data())
        let uiStore = UiStore()
        let entry = ImportFolderEntry(
            importer: Importer(addWatchedFolder: { _ in
                Issue.record("a file is never added")
            }),
            uiStore: uiStore
        )

        #expect(entry.take(file) == nil)
        #expect(uiStore.lastError != nil)
        #expect(uiStore.activeSection == .library)
    }

    @Test("every folder of a drop is taken in")
    func everyDroppedFolderIsTakenIn() async throws {
        let folders = [try folder(), try folder()]
        let added = Added()
        let entry = ImportFolderEntry(
            importer: Importer(addWatchedFolder: { await added.add($0) }),
            uiStore: UiStore()
        )
        let providers = folders.map {
            NSItemProvider(
                item: $0.dataRepresentation as NSData,
                typeIdentifier: UTType.fileURL.identifier
            )
        }

        #expect(entry.take(dropped: providers))
        try await Wait.until { await added.paths.count == folders.count }
        #expect(Set(await added.paths) == Set(folders.map(\.pathBytes)))
    }
}

private actor Added {
    var paths: [Data] = []
    func add(_ path: Data) {
        paths.append(path)
        Wait.happened()
    }
}
