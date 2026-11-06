import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
@Suite(.serialized)
struct ReleaseSelectionFailureTests {
    @Test("A release selection failure is not a candidate-wide error")
    func failureDoesNotBecomePaneError() throws {
        let store = ImportStore()
        let key = bridgeReidentifyKey(releaseId: "release")
        store.reIdentifyCandidates[key] = Candidate(
            reIdentifyKey: key,
            releaseId: "release",
            displayName: "Album Title"
        )
        let session = try #require(
            store.beginMetadataApplication(
                key: key,
                application: .pick(MappingFixtures.link),
                steps: nil
            )
        )
        store.metadataApplicationFailed(
            key: key,
            session: session,
            error: DisplayError(line: "Release details unavailable")
        )
        #expect(store.candidate(forKey: key)?.error == nil)
        #expect(store.loadingPick(forKey: key) == nil)
        #expect(
            store.releaseSelectionFailure(forKey: key)?.error.line
                == "Release details unavailable"
        )
    }

    @Test("A release refused for its track count says both counts on its row")
    func trackCountRefusalStatesBothCounts() async throws {
        let groups = PreviewData.searchStateFoundExact.identifiedGroups
        let pressing = try #require(groups.first?.pressings.first)
        let category = BridgeErrorCategory.metadataTrackCount(
            folderTracks: 12,
            releaseTracks: 10
        )
        let failure = try await failedSelection(
            pressing: pressing,
            error: .Diagnostic(
                category: category,
                detail: "12 tracks in the folder, 10 on the release"
            )
        )
        #expect(failure.error.line == category.localizedLine)
        #expect(failure.error.line.contains("12"))
        #expect(failure.error.line.contains("10"))
        #expect(failure.error.detail == nil)
    }

    @Test("Retry replaces the failed selection and ignores an older completion")
    func retryReplacesFailure() throws {
        let store = MappingFixtures.store(mapping: nil)
        let key = MappingFixtures.candidateKey
        let first = try #require(
            store.beginMetadataApplication(
                key: key,
                application: .pick(MappingFixtures.link),
                steps: nil
            )
        )
        store.metadataApplicationFailed(
            key: key,
            session: first,
            error: DisplayError(line: "First failure")
        )
        store.applyCandidateDetail(
            key: key,
            detail: MappingFixtures.detail(mapping: nil)
        )
        #expect(
            store.releaseSelectionFailure(forKey: key)?.error.line
                == "First failure"
        )
        let retry = try #require(
            store.beginMetadataApplication(
                key: key,
                application: .pick(MappingFixtures.link),
                steps: nil
            )
        )
        #expect(store.releaseSelectionFailure(forKey: key) == nil)
        store.metadataApplicationFailed(
            key: key,
            session: first,
            error: DisplayError(line: "Stale failure")
        )
        #expect(store.metadataApplicationSession(forKey: key) === retry)
        #expect(store.releaseSelectionFailure(forKey: key) == nil)
    }

    /// Every row of the list is handed the failure; only the failed
    /// pressing's row takes it, and its Retry picks that pressing again.
    /// Where the line sits under the row is the `import-search-row-failed`
    /// shot's, and where Retry sits is no longer tested.
    @Test(
        "The error appears once beneath the failed pressing with a working Retry"
    )
    func rowOwnsFailureAndRetry() async throws {
        let pressings = PreviewData.searchStateFoundExact.identifiedGroups
            .flatMap(\.pressings)
        let pressing = try #require(pressings.first)
        let failure = try await failedSelection(
            pressing: pressing,
            error: .Diagnostic(
                category: .import,
                detail: "Provider returned 500"
            )
        )
        var selected: Pressing?
        let owning = try #require(
            row(pressing, failure: failure) { selected = $0 }.rowFailure
        )
        #expect(owning.error.line == failure.error.line)
        for other in pressings.dropFirst() {
            #expect(row(other, failure: failure) { _ in }.rowFailure == nil)
        }
        owning.retry()
        #expect(selected?.link == pressing.link)
    }

    /// Copy details copies the error's whole detail, whose start the row's
    /// line shows; where that button sits is no longer tested.
    @Test(
        "unexpected details are visible and copying retains the entire diagnostic",
        arguments: [
            "Unsupported artwork input",
            "Unsupported artwork input\n"
                + String(
                    repeating: "Decoder context and image information. ",
                    count: 30
                ) + "Terminal cause",
        ]
    )
    func unexpectedDetailsCanBeCopied(_ diagnostic: String) async throws {
        let pressing = try #require(
            PreviewData.searchStateFoundExact.identifiedGroups.first?
                .pressings.first
        )
        let failure = try await failedSelection(
            pressing: pressing,
            error: .Diagnostic(category: .internal, detail: diagnostic)
        )
        let shown = try #require(
            row(pressing, failure: failure) { _ in }.rowFailure
        )
        #expect(shown.error.detail == diagnostic)
        #expect(
            shown.error.detailSummary?.contains("Unsupported artwork input")
                == true
        )
        #expect(!shown.error.line.isEmpty)
    }

    /// A failure whose detail core says repeats its line has no detail to
    /// show, so it draws no Copy details, which the disclosure offers only
    /// for a detail; the row still offers its Retry.
    @Test(
        "a release failure whose line says it all shows Retry without diagnostic controls"
    )
    func failureLineSayingItAllHasNoCopyControl() async throws {
        let pressing = try #require(
            PreviewData.searchStateFoundExact.identifiedGroups.first?
                .pressings.first
        )
        let detail =
            "discs [1, 3] of the release fit the folder's audio equally"
        let failure = try await failedSelection(
            pressing: pressing,
            error: .Diagnostic(
                category: .tiedMediums(mediums: [1, 3]),
                detail: detail
            )
        )
        #expect(failure.error.detail == nil)
        let shown = try #require(
            row(pressing, failure: failure) { _ in }.rowFailure
        )
        #expect(!shown.error.line.contains(detail))
    }
}

extension ReleaseSelectionFailureTests {
    private func failedSelection(pressing: Pressing, error: BridgeError)
        async throws -> ReleaseSelectionFailure
    {
        let store = MappingFixtures.store(mapping: nil)
        let importer = Importer(applyCandidateExternalMetadata: { _, _, _ in
            throw error
        })
        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            application: .pick(pressing.link)
        )
        try await Wait.until {
            store.releaseSelectionFailure(forKey: MappingFixtures.candidateKey)
                != nil
        }
        return try #require(
            store.releaseSelectionFailure(forKey: MappingFixtures.candidateKey)
        )
    }

    private func row(
        _ pressing: Pressing,
        failure: ReleaseSelectionFailure? = nil,
        onRetryUnread: (() -> Void)? = nil,
        onSelect: @escaping (Pressing) -> Void
    ) -> ImportSearchResultRow {
        ImportSearchResultRow(
            pressing: pressing,
            isImporting: false,
            libraryStatus: nil,
            isSelected: false,
            failure: failure,
            onRetryUnread: onRetryUnread,
            onSelect: onSelect
        )
    }
}

/// A row whose document identification could not read shows the same
/// failure line a failed pick does, with a Retry that identifies again;
/// where that Retry sits is no longer tested.
extension ReleaseSelectionFailureTests {
    @MainActor
    @Test(
        "a row whose document could not be read says so, and Retry identifies again"
    )
    func unreadRowSaysSoAndRetriesTheRun() async throws {
        let offered = try #require(
            PreviewData.searchStateFoundExact.identifiedGroups.first?.pressings
                .first
        )
        let pressing = try #require(
            Pressing(
                bridge: BridgePressing(
                    releases: offered.releases,
                    sources: offered.sources,
                    labels: offered.labels,
                    documentFailure: .timeout,
                    pick: offered.link
                )
            )
        )
        let message = BridgeIdentifyFailure.releaseDetails(failure: .timeout)
            .badgeLine
        var retried = false
        var selected: Pressing?
        let shown = try #require(
            row(
                pressing,
                onRetryUnread: { retried = true },
                onSelect: { selected = $0 }
            )
            .rowFailure
        )
        #expect(shown.error.line == message)
        shown.retry()
        #expect(retried)
        #expect(selected == nil)
    }
}
