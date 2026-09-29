import XCTest

/// Edit ▸ Select All and Command-A in the running app: in the import list they
/// select every row the list shows, past the rows its pages have loaded, and
/// in a text field they select the field's text.
final class EditMenuSelectAllTests: XCTestCase {
    /// More rows than the list loads in one page.
    private static let albumCount = 120

    @MainActor
    func testSelectAllReachesEveryListedRowAndStillSelectsText() throws {
        let app = try launchWatching(albums: Self.albumCount)
        let selectedAll = app.staticTexts["\(Self.albumCount) selected"]

        let list = app.outlines.firstMatch
        // The rows are listed newest first, so wait for the first scanned and
        // the last: then every album is in the list.
        for album in ["Album 000", "Album \(Self.albumCount - 1)"] {
            XCTAssertTrue(list.staticTexts[album].waitForExistence(timeout: 60))
        }
        // The top row, wherever the list's order put it.
        let topRow = list.coordinate(withNormalizedOffset: .zero)
            .withOffset(CGVector(dx: 120, dy: 30))

        topRow.click()
        app.typeKey("a", modifierFlags: .command)
        XCTAssertTrue(selectedAll.waitForExistence(timeout: 10))

        // One row selected: the all-rows selection is gone.
        topRow.click()
        XCTAssertTrue(selectedAll.waitForNonExistence(timeout: 10))
        app.menuBars.menuBarItems["Edit"].click()
        app.menuBars.menuItems["Select All"].click()
        XCTAssertTrue(selectedAll.waitForExistence(timeout: 10))

        let filter = app.textFields
            .matching(
                NSPredicate(format: "placeholderValue == %@", "Filter...")
            )
            .firstMatch
        XCTAssertTrue(filter.waitForExistence(timeout: 10))
        filter.click()
        filter.typeText("Album")
        app.typeKey("a", modifierFlags: .command)
        filter.typeText("Z")
        XCTAssertEqual(filter.value as? String, "Z")
    }

    /// Launch the app on a library of its own that watches `albums` folders of
    /// one short WAV each.
    @MainActor
    private func launchWatching(albums: Int) throws -> XCUIApplication {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let home = root.appendingPathComponent("home", isDirectory: true)
        let music = root.appendingPathComponent("music", isDirectory: true)
        try FileManager.default.createDirectory(
            at: home,
            withIntermediateDirectories: true
        )
        for index in 0..<albums {
            let album = music.appendingPathComponent(
                String(format: "Album %03d", index),
                isDirectory: true
            )
            try FileManager.default.createDirectory(
                at: album,
                withIntermediateDirectories: true
            )
            // A different length each, so no two folders share their content.
            try Self.silentWAV(samples: 800 + index)
                .write(to: album.appendingPathComponent("01 Track.wav"))
        }
        addTeardownBlock {
            try FileManager.default.removeItem(at: root)
        }

        let app = XCUIApplication()
        app.launchEnvironment["HOME"] = home.path
        app.launchEnvironment["BAE_UI_TESTING"] = "1"
        app.launchEnvironment["BAE_UI_TESTING_CREATE_LIBRARY"] = "1"
        app.launchEnvironment["BAE_UI_TESTING_WATCH_FOLDER"] = music.path
        app.launch()
        app.activate()
        addTeardownBlock { app.terminate() }
        return app
    }

    /// A mono 16-bit 8 kHz WAV of `samples` silent samples.
    private static func silentWAV(samples: Int) -> Data {
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
}
