import AppKit
import SwiftUI
import XCTest

@testable import bae

final class CandidateFolderLineTests: XCTestCase {
    @MainActor
    func testHeaderStatesCurrentPlacement() async throws {
        let size = NSSize(width: 520, height: 80)
        let (_, placedHost) = SnapshotTestSupport.hostInWindow(
            CandidateFolderLine(
                tab: .pending,
                folderName: "Release Folder",
                folderPaths: ["/library/release-folder"],
                onNavigateToPlacement: {}
            )
            .padding()
            .frame(width: size.width, height: size.height),
            size: size
        )
        let (_, unplacedHost) = SnapshotTestSupport.hostInWindow(
            CandidateFolderLine(
                tab: nil,
                folderName: "Release Folder",
                folderPaths: ["/library/release-folder"],
                onNavigateToPlacement: {}
            )
            .padding()
            .frame(width: size.width, height: size.height),
            size: size
        )

        XCTAssertEqual(
            CandidateFolderLine.label(for: .pending),
            "Pending"
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
