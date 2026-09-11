import AppKit
import BaeKit
import SwiftUI
import Testing
import Vision

@testable import bae

@MainActor
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
            error: "Release details unavailable"
        )
        #expect(store.candidate(forKey: key)?.error == nil)
        #expect(store.loadingReleaseId(forKey: key) == nil)
        #expect(
            store.releaseSelectionFailure(forKey: key)?.message
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
            error: "First failure"
        )
        store.applyCandidateDetail(
            key: key,
            detail: MappingFixtures.detail(mapping: nil)
        )
        #expect(
            store.releaseSelectionFailure(forKey: key)?.message
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
            error: "Stale failure"
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
        let message = "Release details unavailable"
        state.releaseSelectionFailure = ReleaseSelectionFailure(
            release: BridgeMetadataRef(
                source: pressing.lead.source,
                releaseId: pressing.lead.releaseId
            ),
            message: message
        )
        var selected: Pressing?
        let size = NSSize(width: 900, height: 620)
        // Render the production result list and invoke the failed row's Retry.
        let (window, host) = SnapshotTestSupport.hostInWindow(
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
        )
        defer {
            window.contentView = nil
            window.orderOut(nil)
        }
        let png = try await SnapshotTestSupport.capturePNG(host, size: size)
        let observations = try await SnapshotTestSupport.recognizedText(in: png)
        try verifyFailure(observations, message: message, pressing: pressing)
        try clickRetry(observations, window: window, host: host, size: size)
        #expect(selected?.provenance == pressing.provenance)
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

    private func clickRetry(
        _ observations: [SnapshotTestSupport.RecognizedLine],
        window: NSWindow,
        host: NSView,
        size: NSSize
    ) throws {
        let retry = try #require(
            observations.first {
                $0.text.contains(String(localized: "Retry"))
            }
        )
        let point = NSPoint(
            x: retry.boundingBox.midX * size.width,
            y: retry.boundingBox.midY * size.height
        )
        if let control = SnapshotTestSupport.descendants(of: host)
            .compactMap({ $0 as? NSControl })
            .first(where: {
                $0.isEnabled && $0.convert($0.bounds, to: nil).contains(point)
            })
        {
            control.performClick(nil)
        }
        else {
            for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
                window.sendEvent(
                    try #require(
                        NSEvent.mouseEvent(
                            with: type,
                            location: point,
                            modifierFlags: [],
                            timestamp: ProcessInfo.processInfo.systemUptime,
                            windowNumber: window.windowNumber,
                            context: nil,
                            eventNumber: 0,
                            clickCount: 1,
                            pressure: type == .leftMouseDown ? 1 : 0
                        )
                    )
                )
            }
        }
    }
}
