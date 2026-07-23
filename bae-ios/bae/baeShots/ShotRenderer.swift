import SwiftUI
import UIKit

/// Renders a `ShotScene` offscreen into a deterministic 3x PNG. The view is
/// hosted in a dark-appearance key window, laid out, given time for its async
/// content to settle, then captured at a fixed scale so the pixels are the same
/// on any simulator the harness runs on.
@MainActor
enum ShotRenderer {
    /// Backing-store scale for every capture (a current-generation phone's
    /// native scale). Fixed rather than read from the device so the output is
    /// device-independent.
    static let scale: CGFloat = 3

    static func renderPNG(_ scene: ShotScene) async throws -> Data {
        let bounds = CGRect(origin: .zero, size: scene.size)

        let host = UIHostingController(rootView: scene.makeView())
        host.overrideUserInterfaceStyle = .dark
        host.view.frame = bounds

        let window = UIWindow(frame: bounds)
        window.overrideUserInterfaceStyle = .dark
        window.rootViewController = host
        window.makeKeyAndVisible()

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
