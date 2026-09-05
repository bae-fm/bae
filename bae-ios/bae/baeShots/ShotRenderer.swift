import BaeKit
import SwiftUI
import UIKit

/// Renders a `ShotScene` offscreen into a deterministic 3x PNG. The view is
/// hosted in a key window with the requested appearance, given time for its async
/// content to settle, then captured at a fixed scale so the pixels are the same
/// on any simulator the harness runs on.
@MainActor
enum ShotRenderer {
    /// Backing-store scale for every capture (a current-generation phone's
    /// native scale). Fixed rather than read from the device so the output is
    /// device-independent.
    static let scale: CGFloat = 3

    static func renderPNG(
        _ scene: ShotScene,
        mode: AppearanceMode,
        tone: SurfaceTone,
        accent: AccentChoice
    ) async throws -> Data {
        let suite = "fm.bae.appearance-shots"
        guard let defaults = UserDefaults(suiteName: suite) else {
            preconditionFailure("Cannot create screenshot preferences")
        }
        defaults.set(mode.rawValue, forKey: "appearance.mode")
        defaults.set(accent.rawValue, forKey: "appearance.accent")
        defaults.set(tone.rawValue, forKey: "appearance.tone")
        defer { defaults.removePersistentDomain(forName: suite) }
        let bounds = CGRect(origin: .zero, size: scene.size)

        let host = UIHostingController(
            rootView: scene.makeView().appAppearance().defaultAppStorage(defaults)
        )
        host.overrideUserInterfaceStyle = mode == .dark ? .dark : .light
        host.view.frame = bounds

        let window = UIWindow(frame: bounds)
        window.overrideUserInterfaceStyle = mode == .dark ? .dark : .light
        window.rootViewController = host
        window.makeKeyAndVisible()
        defer { window.isHidden = true }

        await settle(view: host.view)

        let format = UIGraphicsImageRendererFormat()
        format.scale = scale
        format.opaque = true
        let renderer = UIGraphicsImageRenderer(bounds: bounds, format: format)
        let image = renderer.image { _ in
            host.view.drawHierarchy(in: bounds, afterScreenUpdates: true)
        }

        guard let data = image.pngData() else {
            throw ShotError.pngEncodingFailed(scene.id)
        }
        return data
    }

    /// Suspend the capturing task so SwiftUI commits layout and any
    /// `.task`-driven work (seeded release loads, grid pages) runs to completion
    /// on the main actor before the capture. Blocking the main thread here would
    /// starve those cooperative tasks — the awaits are what let them run.
    private static func settle(view: UIView) async {
        view.setNeedsLayout()
        view.layoutIfNeeded()
        for _ in 0..<20 {
            await Task.yield()
        }
        try? await Task.sleep(nanoseconds: 1_500_000_000)
        view.setNeedsLayout()
        view.layoutIfNeeded()
    }
}

enum ShotError: Error, CustomStringConvertible {
    case missingDocumentsDir
    case pngEncodingFailed(String)

    var description: String {
        switch self {
        case .missingDocumentsDir:
            return "could not locate the app's Documents directory"
        case .pngEncodingFailed(let id):
            return "failed to PNG-encode scene \(id)"
        }
    }
}
