import XCTest

/// View ▸ Go to Now Playing shows the playing track's row in its album's
/// open detail: here the album is at the bottom of a long grid, and the track
/// is near the end of a long album.
final class GoToNowPlayingTests: XCTestCase {
    private static let albums = LibraryFixture.Album.numbered(300)
    private static let playingTracks = (1...40)
        .map {
            LibraryFixture.PlayingTrack(
                title: String(format: "Deep Track %02d", $0),
                seconds: 1,
                pregapSeconds: 0
            )
        }
    private static let playingNumber = 38
    /// The grid's sort, oldest added first, which puts the playing album,
    /// the newest, last: the JSON the app stores, as property list data.
    private static let oldestFirst: String = {
        let json = #"[{"field":"dateAdded","direction":"ascending"}]"#
        let hex = json.utf8.map { String(format: "%02x", $0) }.joined()
        return "<\(hex)>"
    }()

    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testGoToNowPlayingScrollsToTheTracksRowFarDown() throws {
        let app = try launchApp(
            library: LibraryFixture(
                albums: Self.albums,
                playing: LibraryFixture.Playing(
                    title: "Playing Album",
                    artists: ["Artist"],
                    tracks: Self.playingTracks,
                    track: Self.playingNumber,
                    positionMs: 0
                )
            ),
            defaults: ["librarySortCriteria": Self.oldestFirst]
        )
        let window = app.windows.firstMatch
        let grid = AlbumGrid(app: app)
        let playing = Self.playingTracks[Self.playingNumber - 1].title
        XCTAssertTrue(window.buttons[playing].waitForExistence(timeout: 30))
        let oldest = Self.albums[0].title
        XCTAssertTrue(waitUntil(timeout: 30) { grid.shows(oldest) })
        XCTAssertFalse(grid.shows("Playing Album"))

        app.typeKey("l", modifierFlags: .command)
        XCTAssertTrue(
            waitUntil(timeout: 10) { grid.shows(playing) },
            "the playing track's row is scrolled into view"
        )
    }
}
