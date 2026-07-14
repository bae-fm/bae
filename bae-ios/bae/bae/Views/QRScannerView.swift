import AVFoundation
import SwiftUI

struct QRScannerView: UIViewControllerRepresentable {
    let onScanned: (String) -> Void
    let onError: (String) -> Void

    func makeUIViewController(context _: Context) -> QRScannerViewController {
        let controller = QRScannerViewController()
        controller.onScanned = onScanned
        controller.onError = onError
        return controller
    }

    func updateUIViewController(_: QRScannerViewController, context _: Context) {}
}

class QRScannerViewController: UIViewController, AVCaptureMetadataOutputObjectsDelegate {
    var onScanned: ((String) -> Void)?
    var onError: ((String) -> Void)?
    private var captureSession: AVCaptureSession?
    private var hasScanned = false

    override func viewDidLoad() {
        super.viewDidLoad()
        setupCamera()
    }

    private func setupCamera() {
        let session = AVCaptureSession()

        guard let device = AVCaptureDevice.default(for: .video) else {
            onError?(String(localized: "Camera not available"))
            return
        }

        let input: AVCaptureDeviceInput
        do {
            input = try AVCaptureDeviceInput(device: device)
        }
        catch {
            onError?(
                String(
                    localized:
                        "Could not create camera input: \(error.displayLine)"
                )
            )
            return
        }

        guard session.canAddInput(input) else {
            onError?(String(localized: "Could not add camera input to session"))
            return
        }
        session.addInput(input)

        let output = AVCaptureMetadataOutput()
        guard session.canAddOutput(output) else {
            onError?(String(localized: "Could not add metadata output to session"))
            return
        }
        session.addOutput(output)
        output.setMetadataObjectsDelegate(self, queue: .main)
        output.metadataObjectTypes = [.qr]

        let previewLayer = AVCaptureVideoPreviewLayer(session: session)
        previewLayer.frame = view.bounds
        previewLayer.videoGravity = .resizeAspectFill
        view.layer.addSublayer(previewLayer)

        captureSession = session

        DispatchQueue.global(qos: .userInitiated)
            .async {
                session.startRunning()
            }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        if let previewLayer = view.layer.sublayers?.first as? AVCaptureVideoPreviewLayer {
            previewLayer.frame = view.bounds
        }
    }

    // `AVCaptureMetadataOutput` invokes this from its delegate queue, so the
    // conformance is `nonisolated`; the metadata-object delegate queue is set
    // to `.main` in `setupCamera`, so the hop back onto the main actor is the
    // same run loop and carries the scanned string across the isolation
    // boundary safely.
    nonisolated func metadataOutput(
        _: AVCaptureMetadataOutput,
        didOutput metadataObjects: [AVMetadataObject],
        from _: AVCaptureConnection
    ) {
        guard
            let qrObject = metadataObjects.first
                as? AVMetadataMachineReadableCodeObject,
            let value = qrObject.stringValue
        else {
            return
        }
        Task { @MainActor in
            guard !hasScanned else {
                return
            }
            hasScanned = true
            captureSession?.stopRunning()
            onScanned?(value)
        }
    }
}
