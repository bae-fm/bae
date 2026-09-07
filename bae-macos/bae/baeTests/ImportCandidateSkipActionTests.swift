import BaeKit
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
    @Test("a failed skip remains selected for retry")
    func failedSkipRemainsSelected() async {
        let store = PreviewData.importTabScene().store
        let uiStore = UiStore()
        let failed = PreviewData.importTabCandidate.key
        let successful = PreviewData.importTabDisagreementCandidate.key
        uiStore.setFolderCandidateSelection([failed, successful])
        store.selection = selection(
            keys: [failed, successful],
            skipTargets: [failed, successful].sorted()
        )
        let importer = Importer(setCandidateSkipped: { key, _ in
            if key == failed { throw CocoaError(.fileWriteNoPermission) }
        })
        await ImportCandidateSkipAction(
            importer: importer,
            importStore: store,
            uiStore: uiStore
        )
        .start()?
        .value
        #expect(uiStore.selectedFolderCandidates == [failed])
        #expect(uiStore.lastError != nil)
    }

    @MainActor
    @Test("the shared action skips exactly the eligible current selection")
    func skipsEligibleCurrentSelectionAndClearsIt() async {
        let importStore = PreviewData.importTabScene().store
        let uiStore = UiStore()
        let recorder = SkippedCandidateRecorder()
        let selected: Set<String> = [
            PreviewData.importTabCandidate.key,
            PreviewData.importTabDisagreementCandidate.key,
            PreviewData.triageRowSkipped.candidateKey,
            "candidate:stale",
        ]
        uiStore.setFolderCandidateSelection(selected)
        importStore.selection = selection(
            keys: selected,
            skipTargets: [
                PreviewData.importTabCandidate.key,
                PreviewData.importTabDisagreementCandidate.key,
            ]
            .sorted()
        )
        let importer = Importer(setCandidateSkipped: { key, skipped in
            #expect(skipped)
            await recorder.record(key)
        })

        await ImportCandidateSkipAction(
            importer: importer,
            importStore: importStore,
            uiStore: uiStore
        )
        .start()?
        .value

        // The action uses exactly the dedicated query's Skip targets, in the
        // order supplied by core, without deriving eligibility from list rows.
        #expect(
            await recorder.keys
                == [
                    PreviewData.importTabCandidate.key,
                    PreviewData.importTabDisagreementCandidate.key,
                ]
                .sorted()
        )
        #expect(
            uiStore.selectedFolderCandidates == [
                PreviewData.triageRowSkipped.candidateKey, "candidate:stale",
            ]
        )
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
        )
        .start()?
        .value

        #expect(await recorder.keys.isEmpty)
    }

    private func selection(
        keys: Set<String>,
        skipTargets: [String]
    ) -> BridgeImportSelection {
        BridgeImportSelection(
            candidateKeys: keys.sorted(),
            offers: [
                BridgeImportCandidateActionOffer(
                    action: .skip,
                    candidates: skipTargets.map {
                        BridgeImportCandidateActionTarget(
                            key: $0,
                            displayName: $0
                        )
                    }
                )
            ],
            canCombine: false
        )
    }

    @Test("Skip selected (%lld) is translated in every shipping locale")
    func skipSelectedHasEveryLocalization() throws {
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
        let skipAll = try #require(
            strings["Skip selected (%lld)"] as? [String: Any]
        )
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
