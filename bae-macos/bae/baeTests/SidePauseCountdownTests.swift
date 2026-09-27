import BaeKit
import Foundation
import Testing

@testable import bae

/// The side-pause countdown in Settings and on the prompt card. Core owns the
/// deadline and starts the next side; the UI only counts down to it.
@MainActor
@Suite("Side-pause countdown")
struct SidePauseCountdownTests {
    private static let resumesAtMs: Int64 = 1_767_225_605_000

    private static let countdown = BridgeSideCountdown(
        resumesAtMs: resumesAtMs
    )

    private static func prompt(
        _ boundary: BridgePauseBoundary
    ) -> BridgeSidePausePrompt {
        BridgeSidePausePrompt(
            id: "pause-1",
            boundary: boundary,
            sideLabel: "A",
            countdown: countdown
        )
    }

    /// `ms` milliseconds before the deadline.
    private static func before(_ ms: Int64) -> Date {
        Date(timeIntervalSince1970: TimeInterval(resumesAtMs - ms) / 1000)
    }

    private static func config(pauseBetweenSides: Bool) -> Config {
        PreviewData.makeConfigStore(
            libraryFullWidth: false,
            pauseBetweenSides: pauseBetweenSides
        )
        .config
    }

    @Test("the countdown choice shows only while pausing between sides is on")
    func pickerFollowsThePauseSetting() {
        #expect(
            SidePauseCountdownPicker.isShown(
                for: Self.config(pauseBetweenSides: true)
            )
        )
        #expect(
            !SidePauseCountdownPicker.isShown(
                for: Self.config(pauseBetweenSides: false)
            )
        )
    }

    @Test("the picker offers Off and each length in order")
    func pickerOffersEveryChoice() {
        #expect(
            BridgeSidePauseCountdown.offered.map(\.seconds)
                == [nil, 5, 15, 30, 45, 60]
        )
        #expect(
            BridgeSidePauseCountdown.label(
                seconds: 5,
                locale: Locale(identifier: "en_US")
            ) == "5 seconds"
        )
    }

    @Test("seconds left round up and stop at zero")
    func secondsLeftRoundUp() {
        let countdown = Self.countdown
        #expect(countdown.secondsLeft(at: Self.before(5_000)) == 5)
        #expect(countdown.secondsLeft(at: Self.before(4_001)) == 5)
        #expect(countdown.secondsLeft(at: Self.before(4_000)) == 4)
        #expect(countdown.secondsLeft(at: Self.before(1)) == 1)
        #expect(countdown.secondsLeft(at: Self.before(0)) == 0)
        #expect(countdown.secondsLeft(at: Self.before(-3_000)) == 0)
    }

    @Test("the card line names the medium and the seconds left")
    func cardLineCountsDown() {
        let side = Self.prompt(.side)
        let disc = Self.prompt(.disc)
        #expect(
            side.countdownLine(Self.countdown, at: Self.before(5_000))
                == "The next side starts in 5 seconds."
        )
        #expect(
            side.countdownLine(Self.countdown, at: Self.before(900))
                == "The next side starts in 1 second."
        )
        #expect(
            disc.countdownLine(Self.countdown, at: Self.before(15_000))
                == "The next disc starts in 15 seconds."
        )
    }

    @Test("the card's checkbox names the kind of boundary that ended")
    func checkboxNamesTheBoundary() {
        #expect(Self.prompt(.side).keepPausingLabel() == "Pause between sides")
        #expect(Self.prompt(.disc).keepPausingLabel() == "Pause between discs")
    }

    @Test("the countdown settings strings ship in every locale")
    func countdownStringsHaveEveryLocalization() throws {
        let catalogURL = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appending(path: "bae/Localizable.xcstrings")
        let catalog = try #require(
            try JSONSerialization.jsonObject(
                with: Data(contentsOf: catalogURL)
            ) as? [String: Any]
        )
        let strings = try #require(catalog["strings"] as? [String: Any])

        func locales(_ key: String) throws -> Set<String> {
            let entry = try #require(strings[key] as? [String: Any])
            let localizations = try #require(
                entry["localizations"] as? [String: Any]
            )
            return Set(localizations.keys)
        }

        let reference = try locales("Pause between sides and discs")
        for key in ["Countdown", "Off"] {
            #expect(try locales(key) == reference, "\(key) is missing locales")
        }
    }
}
