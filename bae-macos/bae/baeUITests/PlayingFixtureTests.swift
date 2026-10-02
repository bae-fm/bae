import XCTest

/// The running app opens on the album its fixture is playing: a CUE image's
/// track with a pregap, paused where the fixture says, which the seek bar
/// shows and Previous steps back from.
final class PlayingFixtureTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testOpensPausedInThePlayingTracksPregap() throws {
        let app = try launchOnThePregapTrack(positionMs: 0)

        let window = app.windows.firstMatch
        // The now-playing bar names the track, paused: it offers Play.
        XCTAssertTrue(
            window.buttons["Pregap Track"].waitForExistence(timeout: 20)
        )
        XCTAssertTrue(window.buttons["Play"].exists)
        // The pregap's countdown: two seconds before the track starts.
        XCTAssertTrue(
            seekBarLabel("-0:02", in: window).waitForExistence(timeout: 20)
        )
    }

    /// Past the first seconds of a track, Previous goes to the track's start,
    /// past its pregap; pressed again there, it goes to the track before.
    @MainActor
    func testPreviousTwiceGoesToTheTracksStartThenTheTrackBefore() throws {
        // Five seconds into the track, after its two of pregap.
        let app = try launchOnThePregapTrack(positionMs: 7_000)

        let window = app.windows.firstMatch
        XCTAssertTrue(
            window.buttons["Pregap Track"].waitForExistence(timeout: 20)
        )
        XCTAssertTrue(
            seekBarLabel("0:05", in: window).waitForExistence(timeout: 20)
        )
        let previous = window.buttons["Previous track"]

        previous.click()
        XCTAssertTrue(
            seekBarLabel("0:00", in: window).waitForExistence(timeout: 10),
            "the track starts over"
        )
        XCTAssertTrue(window.buttons["Pregap Track"].exists)
        // Paused as it was, so the track stays at its start.
        XCTAssertTrue(window.buttons["Play"].exists)

        previous.click()
        XCTAssertTrue(
            window.buttons["Opening Track"].waitForExistence(timeout: 10),
            "the track before plays"
        )
        XCTAssertFalse(window.buttons["Pregap Track"].exists)
    }

    /// Launch with playback paused `positionMs` into the second track's
    /// stream, its pregap then the track, after a track with none.
    @MainActor
    private func launchOnThePregapTrack(
        positionMs: Int
    ) throws -> XCUIApplication {
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
                            seconds: 10,
                            pregapSeconds: 2
                        ),
                    ],
                    track: 2,
                    positionMs: positionMs
                )
            )
        )
        XCTAssertTrue(app.windows.firstMatch.waitForExistence(timeout: 20))
        return app
    }

    /// The seek bar's label reading `text`: its elapsed or remaining time on
    /// the left, the track's duration on the right.
    @MainActor
    private func seekBarLabel(
        _ text: String,
        in window: XCUIElement
    ) -> XCUIElement {
        window.staticTexts
            .matching(NSPredicate(format: "value == %@", text))
            .firstMatch
    }
}
