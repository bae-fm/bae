import Foundation

extension ShotRenderer {
    /// `scene` captured in bae's look on `display`, for a test, which has no
    /// boards' sizes: it renders no component scene as one.
    static func renderInBaeLook(
        _ scene: ShotScene,
        on display: ShotDisplay
    ) async throws -> Capture {
        let sizes = FileManager.default.temporaryDirectory
            .appendingPathComponent("bae-no-board-sizes-\(UUID()).json")
        try Data("{}".utf8).write(to: sizes)
        defer { try? FileManager.default.removeItem(at: sizes) }
        return try await render(
            scene,
            in: BaeShotLook(),
            boardSizes: ShotBoardSizes(contentsOf: sizes),
            on: display
        )
    }
}
