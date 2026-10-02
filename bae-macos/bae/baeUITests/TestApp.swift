import XCTest

/// The albums a UI test opens the app's library on, as bae-core's
/// `LibraryFixture` reads them: in the order they were added, so the last is
/// newest. The app writes them straight into the library's tables when it
/// opens it.
struct LibraryFixture: Encodable {
    struct Album: Encodable {
        let title: String
        /// Who the album is credited to, in credit order.
        let artists: [String]
        let tracks: [String]
    }

    var albums: [Album] = []
}

/// One album folder a UI test has the library watch: its name, which the
/// import list shows, and its tracks' file names, each one short silent WAV.
struct WatchedAlbum {
    let folder: String
    let tracks: [String]
}

extension XCTestCase {
    /// Launch the app on a library of its own, under a fresh `HOME`, holding
    /// `library`'s albums and watching a folder of `watched` albums with
    /// identification off.
    @MainActor
    func launchApp(
        library: LibraryFixture = LibraryFixture(),
        watching watched: [WatchedAlbum] = []
    ) throws -> XCUIApplication {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let home = root.appendingPathComponent("home", isDirectory: true)
        try FileManager.default.createDirectory(
            at: home,
            withIntermediateDirectories: true
        )
        addTeardownBlock {
            try FileManager.default.removeItem(at: root)
        }

        let app = XCUIApplication()
        app.launchEnvironment["HOME"] = home.path
        app.launchEnvironment["BAE_UI_TESTING"] = "1"
        app.launchEnvironment["BAE_UI_TESTING_CREATE_LIBRARY"] = "1"
        if !library.albums.isEmpty {
            let fixture = root.appendingPathComponent("library.json")
            try JSONEncoder().encode(library).write(to: fixture)
            app.launchEnvironment["BAE_UI_TESTING_LIBRARY_FIXTURE"] =
                fixture.path
        }
        if !watched.isEmpty {
            let music = root.appendingPathComponent("music", isDirectory: true)
            try writeFolders(watched, in: music)
            app.launchEnvironment["BAE_UI_TESTING_WATCH_FOLDER"] = music.path
        }
        app.launch()
        app.activate()
        addTeardownBlock { app.terminate() }
        return app
    }
}

private func writeFolders(_ albums: [WatchedAlbum], in music: URL) throws {
    var fileCount = 0
    for album in albums {
        let folder = music.appendingPathComponent(
            album.folder,
            isDirectory: true
        )
        try FileManager.default.createDirectory(
            at: folder,
            withIntermediateDirectories: true
        )
        for track in album.tracks {
            // A different length each, so no two files share their content.
            try silentWAV(samples: 800 + fileCount)
                .write(to: folder.appendingPathComponent("\(track).wav"))
            fileCount += 1
        }
    }
}

/// A mono 16-bit 8 kHz WAV of `samples` silent samples.
private func silentWAV(samples: Int) -> Data {
    let dataSize = UInt32(samples * 2)
    var wav = Data()
    func append<T: FixedWidthInteger>(_ value: T) {
        withUnsafeBytes(of: value.littleEndian) {
            wav.append(contentsOf: $0)
        }
    }
    wav.append(contentsOf: Array("RIFF".utf8))
    append(UInt32(36) + dataSize)
    wav.append(contentsOf: Array("WAVEfmt ".utf8))
    append(UInt32(16))
    append(UInt16(1))
    append(UInt16(1))
    append(UInt32(8000))
    append(UInt32(16000))
    append(UInt16(2))
    append(UInt16(16))
    wav.append(contentsOf: Array("data".utf8))
    append(dataSize)
    wav.append(Data(count: samples * 2))
    return wav
}
