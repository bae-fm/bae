import XCTest

/// The running app opens on the album its fixture is playing: a CUE image's
/// track, paused at the start of its pregap, which the seek bar counts down.
final class PlayingFixtureTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testOpensPausedInThePlayingTracksPregap() throws {
        let app = try launchApp(
            library: LibraryFixture(
                playing: LibraryFixture.Playing(
                    title: "Album Title",
                    artists: ["Artist Name"],
                    tracks: [
                        LibraryFixture.PlayingTrack(
                            title: "Opening Track",
                            seconds: 3,
                            pregapSeconds: 0
                        ),
                        LibraryFixture.PlayingTrack(
                            title: "Pregap Track",
                            seconds: 5,
                            pregapSeconds: 2
                        ),
                    ],
                    track: 2,
                    positionMs: 0
                )
            )
        )

        let window = app.windows.firstMatch
        XCTAssertTrue(window.waitForExistence(timeout: 20))
        // The now-playing bar names the track, paused: it offers Play.
        XCTAssertTrue(
            window.buttons["Pregap Track"].waitForExistence(timeout: 20)
        )
        XCTAssertTrue(window.buttons["Play"].exists)
        // The pregap's countdown: two seconds before the track starts.
        let countdown = window.staticTexts
            .matching(NSPredicate(format: "value == %@", "-0:02"))
            .firstMatch
        XCTAssertTrue(countdown.waitForExistence(timeout: 20))
    }
}
