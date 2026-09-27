import BaeKit
import Foundation
import Testing

@testable import bae

/// The banner states the failure core stored for the pane's last command:
/// why it failed, and what the command was doing where that helps.
@Suite("A pane command's stored failure")
struct PaneFailureTests {
    private static let refused = BridgeError.Diagnostic(
        category: .candidateImportInProgress,
        detail: "the import is already running"
    )

    private static var why: String {
        DisplayError(refused)?.line ?? ""
    }

    private static func line(_ command: BridgePaneCommand) -> String? {
        BridgePaneFailure(command: command, error: refused).line
    }

    @Test("an import command's failure is stated as why it failed")
    func importCommandsStateWhy() {
        #expect(!Self.why.isEmpty)
        for command in [BridgePaneCommand.import, .cancelImport, .mergeArtists]
        {
            #expect(Self.line(command) == Self.why)
        }
    }

    @Test("a change to what identification asks says which it was")
    func choiceChangesSayWhatChanged() {
        #expect(
            Self.line(.changeLookups)
                == String(
                    localized:
                        "Couldn't change what identification looks up: \(Self.why)"
                )
        )
        #expect(
            Self.line(.changeSearchWords)
                == String(
                    localized:
                        "Couldn't change what identification searches by: \(Self.why)"
                )
        )
        #expect(
            Self.line(.changeAgreements)
                == String(
                    localized:
                        "Couldn't change what counts as an agreement: \(Self.why)"
                )
        )
    }

    @Test("reading the file tags says so, unless the track count is why")
    func readingFileTagsSaysSo() {
        #expect(
            Self.line(.readFileTags)
                == String(localized: "Couldn't read file tags: \(Self.why)")
        )
        let trackCount = BridgeError.Diagnostic(
            category: .metadataTrackCount,
            detail: "13 tracks, 12 files"
        )
        #expect(
            BridgePaneFailure(command: .readFileTags, error: trackCount).line
                == DisplayError(trackCount)?.line
        )
    }
}
