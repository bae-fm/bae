import AVFoundation
import SwiftUI

/// Checks (and, when undetermined, requests) camera access, then either runs
/// `present` to show a QR scanner or reports a permission error through `onError`.
/// Shared by every QR-scan entry point so the permission flow and its messages
/// stay identical. Runs its callbacks on the main actor.
@MainActor
enum CameraPermission {
    static func requestThenScan(
        present: @escaping () -> Void,
        onError: @escaping (String) -> Void
    ) {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized:
            present()
        case .notDetermined:
            // The async requestAccess keeps the grant handling on this main
            // actor; the synchronous completion-handler form would capture the
            // non-Sendable `present`/`onError` closures in a @Sendable closure.
            Task { @MainActor in
                if await AVCaptureDevice.requestAccess(for: .video) {
                    present()
                }
                else {
                    onError(
                        String(
                            localized:
                                "Camera permission is required to scan QR codes"
                        )
                    )
                }
            }
        default:
            onError(
                String(
                    localized:
                        "Camera access is denied. Enable it in Settings to scan QR codes."
                )
            )
        }
    }
}
