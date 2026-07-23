import Foundation
import Testing

@testable import bae

/// Captures every registered scene to `<id>@ios.png` in the app container's
/// `Documents/shots`, which `scripts/shots/ios.sh` copies out to the gallery
/// output directory (the simulator sandbox can't write to a host path). Every
/// scene is attempted; any failure fails the run with the full list, so a
/// broken scene can never be a silent skip.
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
                let url = out.appendingPathComponent("\(scene.id)@ios.png")
                try data.write(to: url)
            }
            catch {
                failures.append("\(scene.id): \(error)")
            }
        }

        #expect(failures.isEmpty, "scene capture failed: \(failures)")
    }

    /// A fresh `Documents/shots` directory. Cleared first so a scene removed
    /// from the registry can't linger from a previous run into the copy-out.
    static func outputDirectory() throws -> URL {
        guard
            let docs = FileManager.default
                .urls(
                    for: .documentDirectory,
                    in: .userDomainMask
                )
                .first
        else {
            throw ShotError.missingDocumentsDir
        }
        let dir = docs.appendingPathComponent("shots", isDirectory: true)
        try? FileManager.default.removeItem(at: dir)
        try FileManager.default.createDirectory(
            at: dir,
            withIntermediateDirectories: true
        )
        return dir
    }
}
