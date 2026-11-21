import CoreGraphics
import Foundation

/// The size of each component scene's board, in points: the preview row's
/// cell, which `builds shots` measures and writes as `board-sizes.json`
/// before the capture starts, naming the file in `BUILDS_SHOTS_BOARD_SIZES`
/// (`TEST_RUNNER_BUILDS_SHOTS_BOARD_SIZES` through Xcode).
enum ShotBoardSizes {
    private struct Entry: Decodable {
        let width: Double
        let height: Double
    }

    private static let sizes: Result<[String: CGSize], Error> = Result {
        guard
            let path = ProcessInfo.processInfo
                .environment["BUILDS_SHOTS_BOARD_SIZES"]
        else {
            throw ShotError.missingBoardSizes
        }
        let data = try Data(contentsOf: URL(fileURLWithPath: path))
        return try JSONDecoder().decode([String: Entry].self, from: data)
            .mapValues { CGSize(width: $0.width, height: $0.height) }
    }

    /// The board size of component scene `id`; a scene with no entry fails.
    static func size(of id: String) throws -> CGSize {
        guard let size = try sizes.get()[id] else {
            throw ShotError.noBoardSize(id)
        }
        return size
    }
}
