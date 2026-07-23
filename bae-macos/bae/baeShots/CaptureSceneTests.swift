import Foundation
import Testing

@testable import bae

/// Captures every registered scene to `<id>@macos.png` in the directory the
/// harness passes through `TEST_RUNNER_BAE_SHOTS_OUT`. Every scene is attempted;
/// any failure — a scene that fails to render, encode, or write — fails the run
/// with the full list, so a broken scene can never be a silent skip.
@MainActor
struct CaptureSceneTests {
    @Test
    func captureAllScenes() async throws {
        let out = try Self.outputDirectory()

        var failures: [String] = []
        for scene in ShotScene.all {
            do {
                let data = try await ShotRenderer.renderPNG(scene)
                guard !data.isEmpty else {
                    failures.append("\(scene.id): rendered no bytes")
                    continue
                }
                let url = out.appendingPathComponent("\(scene.id)@macos.png")
                try data.write(to: url)
            }
            catch {
                failures.append("\(scene.id): \(error)")
            }
        }

        #expect(failures.isEmpty, "scene capture failed: \(failures)")
    }

    /// The output directory, from `BAE_SHOTS_OUT`. Absent → fail loud: the
    /// harness has nowhere to write and must not silently no-op.
    static func outputDirectory() throws -> URL {
        guard let path = ProcessInfo.processInfo.environment["BAE_SHOTS_OUT"],
            !path.isEmpty
        else {
            throw ShotError.missingOutputDir
        }
        let url = URL(fileURLWithPath: path, isDirectory: true)
        try FileManager.default.createDirectory(
            at: url,
            withIntermediateDirectories: true
        )
        return url
    }
}
