import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Import release boundary rendering")
struct ImportReleaseBoundaryViewTests {
    @MainActor
    @Test("clicking an ambiguity title cannot hide its evidence")
    func ambiguityCannotHideItsEvidence() async throws {
        let size = NSSize(width: 360, height: 320)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            FolderReleaseBoundaryRow(
                boundary: PreviewData.folderReleaseBoundary,
                onDecision: { _, _ in }
            )
            .frame(
                maxWidth: .infinity,
                maxHeight: .infinity,
                alignment: .topLeading
            ),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        let before = try await SnapshotTestSupport.capturePNG(host, size: size)

        let point = NSPoint(x: 24, y: size.height - 18)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let event = try #require(
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
            window.sendEvent(event)
        }
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        let after = try await SnapshotTestSupport.capturePNG(host, size: size)
        #expect(before == after)
    }

    @MainActor
    @Test(
        "choosing an ambiguity decision clears the selected release immediately"
    )
    func decisionClearsSelectionBeforeCoreCompletes() async throws {
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.pending)
        uiStore.setImportCandidateFilterText("Box")
        uiStore.setFolderCandidateSelection([
            "candidate:selected-before-decision"
        ])
        let importer = Importer(
            setFolderReleaseDecision: { _, _ in
                try await Task.sleep(for: .seconds(30))
            }
        )
        let size = NSSize(width: 1200, height: 760)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportView()
                .environment(uiStore)
                .importPreviewEnvironment()
                .environment(uiStore)
                .environment(Library.stub())
                .environment(PreviewAudio.stub())
                .environment(PreviewData.releaseQueueImportStore)
                .environment(importer),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        let point = NSPoint(x: 105, y: size.height - 238)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let event = try #require(
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
            window.sendEvent(event)
        }
        #expect(uiStore.selectedFolderCandidates.isEmpty)
    }

    @Test("scan status is translated in every shipping locale")
    func scanStatusHasEveryLocalization() throws {
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
        let reference = try #require(
            strings["Refresh"] as? [String: Any]
        )
        let referenceLocales = try #require(
            reference["localizations"] as? [String: Any]
        )
        let scanning = try #require(
            strings["Scanning\u{2026}"] as? [String: Any]
        )
        let scanningLocales = try #require(
            scanning["localizations"] as? [String: Any]
        )
        #expect(Set(scanningLocales.keys) == Set(referenceLocales.keys))
    }
}
