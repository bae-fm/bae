import AVFoundation
import SwiftUI
import os.log

private let logger = Logger.bae("QRScanner")

/// Live camera QR-code scanner. Requests camera access on appear, runs an
/// `AVCaptureSession` feeding an `AVCaptureMetadataOutput`, and calls `onScan`
/// with the first decoded payload. When the camera is unavailable or access is
/// denied it renders an explanatory placeholder instead — every caller also
/// offers a paste fallback, so a missing camera is never a dead end.
struct QRScannerView: View {
    let onScan: (String) -> Void

    enum CameraState {
        case requesting
        case ready(AVCaptureSession)
        case denied
        case unavailable
    }

    @State
    private var state: CameraState = .requesting

    var body: some View {
        Group {
            switch state {
            case .requesting:
                placeholder(
                    systemImage: "camera",
                    message: String(localized: "Starting camera...")
                )
            case .ready(let session):
                CameraPreview(session: session, onScan: onScan)
            case .denied:
                placeholder(
                    systemImage: "camera.metering.none",
                    message: String(
                        localized:
                            "Camera access is off. Enable it in System Settings, or paste the code below."
                    )
                )
            case .unavailable:
                placeholder(
                    systemImage: "camera.metering.none",
                    message: String(
                        localized:
                            "No camera available. Paste the code below instead."
                    )
                )
            }
        }
        .task { await start() }
        .onDisappear { stop() }
    }

    private func placeholder(
        systemImage: String,
        message: String
    ) -> some View {
        VStack(spacing: 8) {
            Image(systemName: systemImage)
                .font(.largeTitle)
                .foregroundStyle(.secondary)
            Text(message)
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color.secondary.opacity(0.1))
    }

    private func start() async {
        let status = AVCaptureDevice.authorizationStatus(for: .video)
        switch status {
        case .authorized:
            configureSession()
        case .notDetermined:
            let granted = await AVCaptureDevice.requestAccess(for: .video)
            if granted {
                configureSession()
            }
            else {
                state = .denied
            }
        case .denied, .restricted:
            state = .denied
        @unknown default:
            logger.warning(
                "Unknown camera authorization status: \(status.rawValue)"
            )
            state = .denied
        }
    }

    private func configureSession() {
        guard
            let device = AVCaptureDevice.default(
                .builtInWideAngleCamera,
                for: .video,
                position: .unspecified
            )
        else {
            logger.info("No built-in camera found; QR scanner unavailable")
            state = .unavailable
            return
        }
        let session = AVCaptureSession()
        do {
            let input = try AVCaptureDeviceInput(device: device)
            guard session.canAddInput(input) else {
                logger.error("Capture session refused the camera input")
                state = .unavailable
                return
            }
            session.addInput(input)
        }
        catch {
            logger.error(
                "Failed to open camera input: \(error.localizedDescription)"
            )
            state = .unavailable
            return
        }
        let output = AVCaptureMetadataOutput()
        guard session.canAddOutput(output) else {
            logger.error("Capture session refused the metadata output")
            state = .unavailable
            return
        }
        session.addOutput(output)
        guard output.availableMetadataObjectTypes.contains(.qr) else {
            logger.error("Capture session does not offer QR metadata scanning")
            state = .unavailable
            return
        }
        output.metadataObjectTypes = [.qr]
        state = .ready(session)
    }

    private func stop() {
        if case .ready(let session) = state {
            let toStop = session
            DispatchQueue.global(qos: .userInitiated)
                .async {
                    if toStop.isRunning {
                        toStop.stopRunning()
                    }
                }
        }
    }
}

/// A sheet that hosts `QRScannerView` for code entry where the surrounding
/// surface is a text field rather than a dedicated capture pane. Reports the
/// first decoded code and is otherwise dismissable, so the paste field behind it
/// remains the fallback.
struct InviteScannerSheet: View {
    let onScan: (String) -> Void
    let onDismiss: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Scan invite code")
                    .font(.headline)
                Spacer()
                Button("Cancel") { onDismiss() }
                    .buttonStyle(.borderless)
            }
            .padding()

            Divider()

            QRScannerView(onScan: onScan)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(width: 400, height: 440)
    }
}

/// `AVCaptureVideoPreviewLayer` host. Starts the session when shown, wires the
/// metadata output to a coordinator that forwards the first decoded QR payload.
private struct CameraPreview: NSViewRepresentable {
    let session: AVCaptureSession
    let onScan: (String) -> Void

    func makeNSView(context: Context) -> PreviewNSView {
        let view = PreviewNSView()
        view.previewLayer.session = session
        view.previewLayer.videoGravity = .resizeAspectFill

        let queue = DispatchQueue(label: "fm.bae.qr-scan")
        for output in session.outputs {
            if let metadata = output as? AVCaptureMetadataOutput {
                metadata.setMetadataObjectsDelegate(
                    context.coordinator,
                    queue: queue
                )
            }
        }

        DispatchQueue.global(qos: .userInitiated)
            .async {
                if !session.isRunning {
                    session.startRunning()
                }
            }
        return view
    }

    func updateNSView(_: PreviewNSView, context _: Context) {}

    func makeCoordinator() -> Coordinator {
        Coordinator(onScan: onScan)
    }

    @MainActor
    final class Coordinator: NSObject,
        AVCaptureMetadataOutputObjectsDelegate
    {
        let onScan: (String) -> Void
        /// First decode wins; later frames are ignored so a held-up code fires
        /// the callback once rather than on every frame.
        private var didScan = false

        init(onScan: @escaping (String) -> Void) {
            self.onScan = onScan
        }

        nonisolated func metadataOutput(
            _: AVCaptureMetadataOutput,
            didOutput metadataObjects: [AVMetadataObject],
            from _: AVCaptureConnection
        ) {
            let codes = metadataObjects.compactMap {
                ($0 as? AVMetadataMachineReadableCodeObject)?.stringValue
            }
            guard let code = codes.first else { return }
            Task { @MainActor in
                guard !self.didScan else { return }
                self.didScan = true
                self.onScan(code)
            }
        }
    }

    /// Layer-backed host whose backing layer is the capture preview, so the
    /// camera fills the view and tracks resizes.
    final class PreviewNSView: NSView {
        let previewLayer = AVCaptureVideoPreviewLayer()

        override init(frame frameRect: NSRect) {
            super.init(frame: frameRect)
            wantsLayer = true
            layer = previewLayer
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            fatalError("init(coder:) is not supported")
        }
    }
}
