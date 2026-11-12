import BaeKit
import Foundation
import Testing

@testable import bae

/// Captures every registered scene to `<board>@macos.png`, with the elements
/// its view lists beside it as `<board>@macos.json`, in the directory the
/// bae-shots tool passes through `TEST_RUNNER_BAE_SHOTS_OUT`. Every scene is
/// attempted; any failure — a scene that fails to render, encode, or write —
/// fails the run with the full list, so a broken scene can never be a silent
/// skip.
///
/// Each scene is drawn once, in the look the boards draw: dark, Midnight, Pink.
/// `BAE_SHOTS_SCENES` (comma-separated board-name prefixes) narrows the run to
/// the scenes a change touches; unset, every scene renders.
///
/// A scene shown in its window brings the app to the front and makes its
/// window key, taking the focus from whatever the person at the machine is
/// doing, so those scenes render only when `BAE_SHOTS_WINDOWS` is `1`, as the
/// screenshot workflow sets it.
@MainActor
struct CaptureSceneTests {
    @Test
    func captureAllScenes() async throws {
        let out = try Self.outputDirectory()

        var failures: [String] = []
        for scene in try Self.selectedScenes() {
            do {
                let capture = try await ShotRenderer.render(scene)
                guard !capture.png.isEmpty else {
                    failures.append("\(scene.id): rendered no bytes")
                    continue
                }
                let url = out.appendingPathComponent("\(scene.id)@macos.png")
                try capture.png.write(to: url)
                try capture.elements.json()
                    .write(
                        to: url.deletingPathExtension()
                            .appendingPathExtension("json")
                    )
            }
            catch {
                failures.append("\(scene.id): \(error)")
            }
        }

        #expect(failures.isEmpty, "scene capture failed: \(failures)")
    }

    /// The scenes whose id starts with one of `BAE_SHOTS_SCENES`'s prefixes,
    /// or every scene when it is unset. A prefix matching no scene fails loud.
    static func selectedScenes() throws -> [ShotScene] {
        let rendered = ShotScene.all.filter { scene in
            switch scene.frame {
            case .content: true
            case .window: Self.rendersWindows
            }
        }
        guard let prefixes = Self.listed("BAE_SHOTS_SCENES") else {
            return rendered
        }
        for prefix in prefixes
        where !rendered.contains(where: { $0.id.hasPrefix(prefix) }) {
            throw ShotError.unknownSelection("scene", prefix)
        }
        return rendered.filter { scene in
            prefixes.contains { scene.id.hasPrefix($0) }
        }
    }

    /// Whether the run renders the scenes shown in their windows.
    private static var rendersWindows: Bool {
        ProcessInfo.processInfo.environment["BAE_SHOTS_WINDOWS"] == "1"
    }

    private static func listed(_ variable: String) -> [String]? {
        guard let value = ProcessInfo.processInfo.environment[variable],
            !value.isEmpty
        else { return nil }
        return value.split(separator: ",").map { String($0) }
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
