import XCTest

/// The state a UI test opens the app's library on, as bae-core's
/// `LibraryFixture` reads it. The app writes it straight into the library's
/// tables as it opens the library, before any of its services starts.
struct LibraryFixture {
    /// One album, as the grid lists it.
    struct Album: Encodable {
        let title: String
        /// Who the album is credited to, in credit order.
        let artists: [String]
        let tracks: [String]
    }

    /// A folder the library watches, named below the test's own folder, and
    /// the candidates the import list lists under it. Nothing is on disk
    /// there: the import list reads as the fixture states it.
    struct WatchedFolder {
        let name: String
        let candidates: [Candidate]
    }

    /// One candidate folder of FLAC tracks, in one of Found's states.
    struct Candidate: Encodable {
        /// The folder's name, which the import list shows.
        let folder: String
        /// Its tracks' file names.
        let tracks: [String]
        let state: CandidateState
    }

    /// A state of Found's that the stored rows hold.
    enum CandidateState: String, Encodable {
        case notLookedUp = "not_looked_up"
        case needsYou = "needs_you"
        case identified
        case unmatched
        case lookupError = "lookup_error"
        case error
        case importError = "import_error"
    }

    /// The library's albums in the order they were added, so the last is
    /// newest.
    var albums: [Album] = []
    var watchedFolders: [WatchedFolder] = []

    var isEmpty: Bool {
        albums.isEmpty && watchedFolders.isEmpty
    }
}

/// A fixture as the app reads it, with every folder at its path.
private struct EncodedFixture: Encodable {
    struct WatchedFolder: Encodable {
        let path: String
        let candidates: [LibraryFixture.Candidate]
    }

    let albums: [LibraryFixture.Album]
    let watchedFolders: [WatchedFolder]

    init(_ fixture: LibraryFixture, under root: URL) {
        albums = fixture.albums
        watchedFolders = fixture.watchedFolders.map { folder in
            WatchedFolder(
                path: root.appendingPathComponent(folder.name).path,
                candidates: folder.candidates
            )
        }
    }
}

extension XCTestCase {
    /// Launch the app on a library of its own, under a fresh `HOME`, holding
    /// the state `library` names.
    @MainActor
    func launchApp(
        library: LibraryFixture = LibraryFixture()
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
        if !library.isEmpty {
            let fixture = root.appendingPathComponent("library.json")
            let encoder = JSONEncoder()
            encoder.keyEncodingStrategy = .convertToSnakeCase
            try encoder.encode(EncodedFixture(library, under: root))
                .write(to: fixture)
            app.launchEnvironment["BAE_UI_TESTING_LIBRARY_FIXTURE"] =
                fixture.path
        }
        app.launch()
        app.activate()
        addTeardownBlock { app.terminate() }
        return app
    }
}
