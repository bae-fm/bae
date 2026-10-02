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

    /// The album playback is on as the app opens, added after `albums`: a
    /// disc image of silent CD audio the launch writes, which a CUE sheet
    /// carves into `tracks`.
    struct Playing {
        let title: String
        let artists: [String]
        let tracks: [PlayingTrack]
        /// The number of the track playback is on, from 1.
        let track: Int
        /// How far into that track's stream playback is: its pregap, then
        /// the track.
        let positionMs: Int
    }

    /// One track of the playing album's disc image.
    struct PlayingTrack {
        let title: String
        /// The track's own audio.
        let seconds: Int
        /// The audio before its INDEX 01 that belongs to it; none at 0.
        let pregapSeconds: Int
    }

    /// The library's albums in the order they were added, so the last is
    /// newest.
    var albums: [Album] = []
    var watchedFolders: [WatchedFolder] = []
    var playing: Playing?

    var isEmpty: Bool {
        albums.isEmpty && watchedFolders.isEmpty && playing == nil
    }
}

/// A fixture as the app reads it, with every folder at its path.
private struct EncodedFixture: Encodable {
    struct WatchedFolder: Encodable {
        let path: String
        let candidates: [LibraryFixture.Candidate]
    }

    struct Playing: Encodable {
        let title: String
        let artists: [String]
        let cueSheet: String
        let track: Int
        let positionMs: Int
    }

    let albums: [LibraryFixture.Album]
    let watchedFolders: [WatchedFolder]
    let playing: Playing?

    /// `fixture` with its folders under `root`, and the playing album's disc
    /// image and sheet written there.
    init(_ fixture: LibraryFixture, under root: URL) throws {
        albums = fixture.albums
        watchedFolders = fixture.watchedFolders.map { folder in
            WatchedFolder(
                path: root.appendingPathComponent(folder.name).path,
                candidates: folder.candidates
            )
        }
        playing = try fixture.playing.map { playing in
            Playing(
                title: playing.title,
                artists: playing.artists,
                cueSheet: try writeDiscImage(playing.tracks, in: root).path,
                track: playing.track,
                positionMs: playing.positionMs
            )
        }
    }
}

/// Samples a second of CD audio.
private let cdSampleRate = 44_100

/// Write a WAV disc image of silent CD audio holding `tracks` one after the
/// other, each its pregap then its own audio, and the CUE sheet carving it,
/// in `folder`. Returns the sheet.
private func writeDiscImage(
    _ tracks: [LibraryFixture.PlayingTrack],
    in folder: URL
) throws -> URL {
    var sheet = "FILE \"disc.wav\" WAVE\n"
    var seconds = 0
    for (index, track) in tracks.enumerated() {
        sheet += String(format: "  TRACK %02d AUDIO\n", index + 1)
        sheet += "    TITLE \"\(track.title)\"\n"
        if track.pregapSeconds > 0 {
            sheet += "    INDEX 00 \(cuePosition(seconds: seconds))\n"
            seconds += track.pregapSeconds
        }
        sheet += "    INDEX 01 \(cuePosition(seconds: seconds))\n"
        seconds += track.seconds
    }
    try silentCDAudio(seconds: seconds)
        .write(to: folder.appendingPathComponent("disc.wav"))
    let sheetFile = folder.appendingPathComponent("disc.cue")
    try sheet.write(to: sheetFile, atomically: true, encoding: .utf8)
    return sheetFile
}

/// A CUE sheet's `mm:ss:ff` for a whole number of seconds.
private func cuePosition(seconds: Int) -> String {
    String(format: "%02d:%02d:00", seconds / 60, seconds % 60)
}

/// A stereo 16-bit WAV of `seconds` of silence at the CD sample rate.
private func silentCDAudio(seconds: Int) -> Data {
    let channels = 2
    let bytesPerSample = 2
    let dataSize = UInt32(seconds * cdSampleRate * channels * bytesPerSample)
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
    append(UInt16(channels))
    append(UInt32(cdSampleRate))
    append(UInt32(cdSampleRate * channels * bytesPerSample))
    append(UInt16(channels * bytesPerSample))
    append(UInt16(bytesPerSample * 8))
    wav.append(contentsOf: Array("data".utf8))
    append(dataSize)
    wav.append(Data(count: Int(dataSize)))
    return wav
}

extension XCTestCase {
    /// Launch the app on a library of its own, under a fresh `HOME`, holding
    /// the state `library` names, on defaults of its own that start as
    /// `defaults`. The app reads and writes those in place of its user
    /// defaults, which are the person's running the test and which `HOME`
    /// does not move.
    @MainActor
    func launchApp(
        library: LibraryFixture = LibraryFixture(),
        defaults: [String: Any] = [:]
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
        var startingDefaults = defaults
        if library.playing != nil {
            // "Restore on launch", without which the app opens on nothing
            // playing whatever the library last played.
            startingDefaults["persistPlayback"] = true
        }
        let defaultsFile = root.appendingPathComponent("defaults.plist")
        try PropertyListSerialization.data(
            fromPropertyList: startingDefaults,
            format: .xml,
            options: 0
        )
        .write(to: defaultsFile)
        app.launchEnvironment["BAE_UI_TESTING_DEFAULTS"] = defaultsFile.path
        app.launch()
        app.activate()
        addTeardownBlock { app.terminate() }
        return app
    }
}

extension XCUIApplication {
    /// The defaults the app runs on, as `launchApp` named them.
    var appDefaults: UserDefaults {
        guard let path = launchEnvironment["BAE_UI_TESTING_DEFAULTS"],
            let defaults = UserDefaults(suiteName: path)
        else {
            preconditionFailure("the app was not launched by launchApp")
        }
        return defaults
    }
}
