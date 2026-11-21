import AVFoundation
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacQRScanner` while the camera starts, with camera access off and
    /// with no camera. The row that shows the camera's picture needs a
    /// camera and has no shot.
    static let macQRScannerScenes: [ShotScene] = [
        qrScanner("mac.component.qr-scanner.starting", .requesting),
        qrScanner("mac.component.qr-scanner.camera-access-off", .denied),
        qrScanner("mac.component.qr-scanner.no-camera", .unavailable),
        qrScanner(
            "mac.component.qr-scanner.camera",
            .ready(
                CameraCapture(
                    session: AVCaptureSession(),
                    output: AVCaptureVideoDataOutput()
                )
            )
        ),
    ]

    private static func qrScanner(
        _ id: String,
        _ state: MacQRScanner.CameraState
    ) -> ShotScene {
        component(id, width: 360, height: 316) {
            MacQRScanner(onScan: { _ in }, showing: state)
                .frame(width: 360, height: 300)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.card))
        }
    }
}
