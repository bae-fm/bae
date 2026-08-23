import Foundation
import Testing

@testable import bae

private actor SkippedCandidateRecorder {
    private(set) var keys: [String] = []

    func record(_ key: String) {
        keys.append(key)
    }
}

@Suite("Import candidate bulk skip")
struct ImportCandidateSkipActionTests {
    @MainActor
    @Test("the shared action skips exactly the eligible current selection")
    func skipsEligibleCurrentSelectionAndClearsIt() async {
        let importStore = PreviewData.importTabScene().store
        let uiStore = UiStore()
        let recorder = SkippedCandidateRecorder()
        let selected: Set<String> = [
            PreviewData.importTabCandidate.key,
            PreviewData.importTabConflictCandidate.key,
            PreviewData.triageRowSkipped.candidateKey,
            "candidate:stale",
        ]
        uiStore.setFolderCandidateSelection(selected)
        let importer = Importer(setCandidateSkipped: { key, skipped in
            #expect(skipped)
            await recorder.record(key)
        })

        await ImportCandidateSkipAction(
            importer: importer,
            importStore: importStore,
            uiStore: uiStore
        ).perform()

        // The eligible pair, and only it: a row already skipped offers Unskip
        // rather than Skip, and a key the list no longer holds offers nothing.
        // The action works through them in key order.
        #expect(
            await recorder.keys
                == [
                    PreviewData.importTabCandidate.key,
                    PreviewData.importTabConflictCandidate.key,
                ].sorted()
        )
        #expect(uiStore.selectedFolderCandidates.isEmpty)
    }

    @MainActor
    @Test("an empty selection has no skip operation")
    func emptySelectionDoesNothing() async {
        let recorder = SkippedCandidateRecorder()
        let importer = Importer(setCandidateSkipped: { key, _ in
            await recorder.record(key)
        })

        await ImportCandidateSkipAction(
            importer: importer,
            importStore: PreviewData.importTabScene().store,
            uiStore: UiStore()
        ).perform()

        #expect(await recorder.keys.isEmpty)
    }

    @Test("Skip All is translated in every shipping locale")
    func skipAllHasEveryLocalization() throws {
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
        let reference = try #require(strings["Skip"] as? [String: Any])
        let referenceLocales = try #require(
            reference["localizations"] as? [String: Any]
        )
        let skipAll = try #require(strings["Skip All"] as? [String: Any])
        let skipAllLocales = try #require(
            skipAll["localizations"] as? [String: Any]
        )

        #expect(Set(skipAllLocales.keys) == Set(referenceLocales.keys))
        for locale in referenceLocales.keys {
            let localization = try #require(
                skipAllLocales[locale] as? [String: Any]
            )
            let unit = try #require(
                localization["stringUnit"] as? [String: Any]
            )
            #expect(unit["state"] as? String == "translated")
            #expect(!(unit["value"] as? String ?? "").isEmpty)
        }
    }
}
