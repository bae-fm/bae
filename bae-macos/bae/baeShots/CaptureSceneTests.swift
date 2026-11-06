import BaeKit
import Foundation
import Testing

@testable import bae

/// Captures every registered scene to `<id>@macos.png` in the directory the
/// harness passes through `TEST_RUNNER_BAE_SHOTS_OUT`. Every scene is attempted;
/// any failure — a scene that fails to render, encode, or write — fails the run
/// with the full list, so a broken scene can never be a silent skip.
///
/// `BAE_SHOTS_SCENES` (comma-separated scene id prefixes) and `BAE_SHOTS_TONES`
/// (comma-separated surface tones) narrow the run to the scenes and tones a
/// change touches; unset, every scene renders in every tone, both modes.
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

        let scenes = try Self.selectedScenes()
        let tones = try Self.selectedTones()

        var failures: [String] = []
        for mode in [AppearanceMode.dark, .light] {
            for tone in tones {
                for scene in scenes {
                    do {
                        let accent: AccentChoice =
                            tone == .standard
                            ? .standard
                            : tone == .plum
                                ? .purple : tone == .slate ? .teal : .blue
                        let data = try await ShotRenderer.renderPNG(
                            scene,
                            mode: mode,
                            tone: tone,
                            accent: accent
                        )
                        guard !data.isEmpty else {
                            failures.append("\(scene.id): rendered no bytes")
                            continue
                        }
                        // The stock look carries no suffix.
                        let suffix =
                            mode == .dark && tone == .standard
                            ? "" : "-\(mode.rawValue)-\(tone.rawValue)"
                        let url = out.appendingPathComponent(
                            "\(scene.id)\(suffix)@macos.png"
                        )
                        try data.write(to: url)
                    }
                    catch {
                        failures.append("\(scene.id): \(error)")
                    }
                }

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

    /// The tones `BAE_SHOTS_TONES` names, or every tone when it is unset.
    static func selectedTones() throws -> [SurfaceTone] {
        guard let names = Self.listed("BAE_SHOTS_TONES") else {
            return SurfaceTone.allCases
        }
        return try names.map { name in
            guard let tone = SurfaceTone(rawValue: name) else {
                throw ShotError.unknownSelection("tone", name)
            }
            return tone
        }
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
