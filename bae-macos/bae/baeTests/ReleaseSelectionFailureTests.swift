import AppKit
import BaeKit
import SwiftUI
import Testing
import Vision

@testable import bae

@MainActor
@Suite(.serialized)
struct ReleaseSelectionFailureTests {
    @Test("A release selection failure is not a candidate-wide error")
    func failureDoesNotBecomePaneError() throws {
        let store = ImportStore()
        let key = "reidentify:release"
        store.reIdentifyCandidates[key] = Candidate(
            reIdentifyKey: key,
            releaseId: "release",
            displayName: "Album Title"
        )
        let session = try #require(
            store.beginMetadataApplication(
                key: key,
                provenance: MappingFixtures.provenance
            )
        )
        store.metadataApplicationFailed(
            key: key,
            session: session,
            error: DisplayError(line: "Release details unavailable")
        )
        #expect(store.candidate(forKey: key)?.error == nil)
        #expect(store.loadingReleaseId(forKey: key) == nil)
        #expect(
            store.releaseSelectionFailure(forKey: key)?.error.line
                == "Release details unavailable"
        )
    }

    @Test("Retry replaces the failed selection and ignores an older completion")
    func retryReplacesFailure() throws {
        let store = MappingFixtures.store(mapping: nil)
        let key = MappingFixtures.candidateKey
        let first = try #require(
            store.beginMetadataApplication(
                key: key,
                provenance: MappingFixtures.provenance
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
                provenance: MappingFixtures.provenance
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

    @Test(
        "The error appears once beneath the failed pressing with a working Retry"
    )
    func rowOwnsFailureAndRetry() async throws {
        var state = PreviewData.searchStateFoundExact
        let pressing = try #require(
            state.identifiedGroups.first?.pressings.first
        )
        state.releaseSelectionFailure = try await failedSelection(
            pressing: pressing,
            error: .Diagnostic(
                category: .import,
                detail: "Provider returned 500"
            )
        )
        let message = try #require(state.releaseSelectionFailure).error.line
        var selected: Pressing?
        let size = NSSize(width: 900, height: 620)
        // Render the production result list and invoke the failed row's Retry.
        try await SnapshotTestSupport.withHostedWindow(
            ReleaseGroupListView(
                groups: state.identifiedGroups,
                isImporting: false,
                libraryStatuses: [:],
                selectedReleaseId: nil,
                loadingReleaseId: nil,
                releaseSelectionFailure: state.releaseSelectionFailure,
                onSelect: { selected = $0 },
                trailing: { EmptyView() }
            )
            .importPreviewEnvironment()
            .background(Theme.background)
            .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            let observations = try await SnapshotTestSupport.recognizedText(
                in: png
            )
            try verifyFailure(
                observations,
                message: message,
                pressing: pressing
            )
            try clickControl(
                String(localized: "Retry"),
                observations: observations,
                window: window,
                size: size
            )
            #expect(selected?.provenance == pressing.provenance)
        }
    }

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
        let state = PreviewData.searchStateFoundExact
        let pressing = try #require(
            state.identifiedGroups.first?.pressings.first
        )
        let failure = try await failedSelection(
            pressing: pressing,
            error: .Diagnostic(category: .internal, detail: diagnostic)
        )
        let size = NSSize(width: 1100, height: 680)
        try await SnapshotTestSupport.withHostedWindow(
            ReleaseGroupListView(
                groups: state.identifiedGroups,
                isImporting: false,
                libraryStatuses: [:],
                selectedReleaseId: nil,
                loadingReleaseId: nil,
                releaseSelectionFailure: failure,
                onSelect: { _ in },
                trailing: { EmptyView() }
            )
            .importPreviewEnvironment()
            .background(Theme.background)
            .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            let observations = try await SnapshotTestSupport.recognizedText(
                in: png
            )
            #expect(
                observations.map(\.text).carrying("Unsupported artwork input")
            )
            #expect(
                observations.map(\.text).carrying(String(localized: "Retry"))
            )
            #expect(observations.map(\.text).carrying(failure.error.line))
            try preservingClipboard { clipboard in
                try clickControl(
                    String(localized: "Copy details"),
                    observations: observations,
                    window: window,
                    size: size
                )
                #expect(clipboard.string(forType: .string) == diagnostic)
            }
        }
    }

    @Test("expected release failures show Retry without diagnostic controls")
    func expectedFailureHasNoCopyControl() async throws {
        let state = PreviewData.searchStateFoundExact
        let pressing = try #require(
            state.identifiedGroups.first?.pressings.first
        )
        let failure = try await failedSelection(
            pressing: pressing,
            error: .Diagnostic(
                category: .import,
                detail: "Provider returned 404"
            )
        )
        #expect(failure.error.detail == nil)
        let size = NSSize(width: 1100, height: 680)
        try await SnapshotTestSupport.withHostedWindow(
            ReleaseGroupListView(
                groups: state.identifiedGroups,
                isImporting: false,
                libraryStatuses: [:],
                selectedReleaseId: nil,
                loadingReleaseId: nil,
                releaseSelectionFailure: failure,
                onSelect: { _ in },
                trailing: { EmptyView() }
            )
            .importPreviewEnvironment()
            .background(Theme.background)
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            let lines = try await SnapshotTestSupport.recognizedText(in: png)
                .map(\.text)
            #expect(lines.carrying(String(localized: "Retry")))
            #expect(!lines.carrying(String(localized: "Copy details")))
            #expect(!lines.carrying("Provider returned 404"))
        }
    }

}

extension ReleaseSelectionFailureTests {
    private func failedSelection(pressing: Pressing, error: BridgeError)
        async throws -> ReleaseSelectionFailure
    {
        let store = MappingFixtures.store(mapping: nil)
        let importer = Importer(applyCandidateExternalMetadata: { _, _ in
            throw error
        })
        ImportSearchFlow.applyMetadata(
            importer: importer,
            importStore: store,
            endEditing: {},
            key: MappingFixtures.candidateKey,
            provenance: pressing.provenance
        )
        try await Wait.until {
            store.releaseSelectionFailure(forKey: MappingFixtures.candidateKey)
                != nil
        }
        return try #require(
            store.releaseSelectionFailure(forKey: MappingFixtures.candidateKey)
        )
    }

    private func verifyFailure(
        _ observations: [SnapshotTestSupport.RecognizedLine],
        message: String,
        pressing: Pressing
    ) throws {
        // Matched by containment: the line draws a warning symbol beside its
        // words, and recognition returns the two glued together.
        func carriesMessage(_ observation: SnapshotTestSupport.RecognizedLine)
            -> Bool
        {
            observation.text.contains(message)
        }
        #expect(observations.filter(carriesMessage).count == 1)
        let errorLine = try #require(observations.first(where: carriesMessage))
        let catalog = try #require(pressing.lead.catalogNumber)
        let facts = try #require(
            observations.first { $0.text.contains(catalog) }
        )
        #expect(errorLine.boundingBox.midY < facts.boundingBox.midY)
    }

    private func clickControl(
        _ label: String,
        observations: [SnapshotTestSupport.RecognizedLine],
        window: NSWindow,
        size: NSSize
    ) throws {
        let retry = try #require(
            observations.first {
                $0.text.contains(label)
            }
        )
        let point = NSPoint(
            x: retry.boundingBox.midX * size.width,
            y: retry.boundingBox.midY * size.height
        )
        try HostedInput.press(at: point, in: window)
    }

    /// Run `body` against the general pasteboard, and put back what the
    /// person running the suite had on it.
    private func preservingClipboard(
        _ body: (NSPasteboard) throws -> Void
    ) throws {
        let clipboard = NSPasteboard.general
        let previous = (clipboard.pasteboardItems ?? [])
            .map { item in
                item.types.compactMap {
                    type -> (NSPasteboard.PasteboardType, Data)? in
                    item.data(forType: type).map { (type, $0) }
                }
            }
        defer {
            clipboard.clearContents()
            let items = previous.map { values in
                let item = NSPasteboardItem()
                for (type, data) in values { item.setData(data, forType: type) }
                return item
            }
            if !items.isEmpty { #expect(clipboard.writeObjects(items)) }
        }
        try body(clipboard)
    }
}
