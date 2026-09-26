import AppKit
import SwiftUI
import XCTest

@testable import bae

final class CandidateFolderLineTests: XCTestCase {
    @MainActor
    func testHeaderStatesCurrentPlacement() async throws {
        let size = NSSize(width: 520, height: 80)
        try await SnapshotTestSupport.withHostedWindow(
            CandidateFolderLine(
                tab: .pending,
                folderName: "Release Folder",
                folderPaths: ["/library/release-folder"],
                onNavigateToPlacement: {}
            )
            .padding()
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, placedHost in
            try await SnapshotTestSupport.withHostedWindow(
                CandidateFolderLine(
                    tab: nil,
                    folderName: "Release Folder",
                    folderPaths: ["/library/release-folder"],
                    onNavigateToPlacement: {}
                )
                .padding()
                .frame(width: size.width, height: size.height),
                size: size
            ) { _, unplacedHost in

                XCTAssertEqual(
                    CandidateFolderLine.label(for: .pending),
                    "Found"
                )
                let placed = try await SnapshotTestSupport.capturePNG(
                    placedHost,
                    size: size
                )
                let unplaced = try await SnapshotTestSupport.capturePNG(
                    unplacedHost,
                    size: size
                )
                XCTAssertNotEqual(placed, unplaced)
            }
        }
    }
}
