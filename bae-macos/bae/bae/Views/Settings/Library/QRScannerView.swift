import AVFoundation
import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("QRScanner")

/// Live camera QR-code scanner. Requests camera access on appear, runs an
/// `AVCaptureSession` feeding camera frames to Vision, and calls `onScan`
/// with the first decoded payload. When the camera is unavailable or access is
/// denied it renders an explanatory placeholder instead — every caller also
/// offers a paste fallback, so a missing camera is never a dead end.
struct QRScannerView: View {
    let onScan: (String) -> Void

    private enum CameraState {
        case requesting
        case ready(CameraCapture)
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
            case .ready(let capture):
                CameraPreview(capture: capture, onScan: onScan)
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
            guard !Task.isCancelled else { return }
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
        guard let device = AVCaptureDevice.default(for: .video) else {
            logger.info("No camera found; QR scanner unavailable")
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
        let output = AVCaptureVideoDataOutput()
        output.alwaysDiscardsLateVideoFrames = true
        guard session.canAddOutput(output) else {
            logger.error("Capture session refused the video output")
            state = .unavailable
            return
        }
        session.addOutput(output)
        state = .ready(CameraCapture(session: session, output: output))
    }

    private func stop() {
        if case .ready(let capture) = state {
            capture.stop()
        }
    }
}

/// Owns the capture graph and serializes its blocking start/stop calls. Closing
/// the scanner queues `stop` after any in-flight `start`, so the camera cannot
/// be left running by a lifecycle race.
private final class CameraCapture: @unchecked Sendable {
    let session: AVCaptureSession
    let output: AVCaptureVideoDataOutput

    private let queue = DispatchQueue(
        label: "fm.bae.qr-capture",
        qos: .userInitiated
    )

    init(session: AVCaptureSession, output: AVCaptureVideoDataOutput) {
        self.session = session
        self.output = output
    }

    func start() {
        queue.async { [self] in
            if !session.isRunning {
                session.startRunning()
            }
        }
    }

    func stop() {
        queue.async { [self] in
            if session.isRunning {
                session.stopRunning()
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
/// video output to a coordinator that forwards the first decoded QR payload.
private struct CameraPreview: NSViewRepresentable {
    let capture: CameraCapture
    let onScan: (String) -> Void

    func makeNSView(context: Context) -> PreviewNSView {
        let view = PreviewNSView()
        view.previewLayer.session = capture.session
        view.previewLayer.videoGravity = .resizeAspectFill

        capture.output.setSampleBufferDelegate(
            context.coordinator,
            queue: context.coordinator.scanQueue
        )
        capture.start()
        return view
    }

    func updateNSView(_: PreviewNSView, context _: Context) {}

    func makeCoordinator() -> Coordinator {
        Coordinator(onScan: onScan)
    }

    final class Coordinator: NSObject,
        AVCaptureVideoDataOutputSampleBufferDelegate,
        @unchecked Sendable
    {
        let scanQueue = DispatchQueue(
            label: "fm.bae.qr-scan",
            qos: .userInitiated
        )
        let onScan: (String) -> Void
        /// First decode wins; later frames are ignored so a held-up code fires
        /// the callback once rather than on every frame.
        private var didScan = false
        private var didLogDecodeFailure = false

        init(onScan: @escaping (String) -> Void) {
            self.onScan = onScan
        }

        func captureOutput(
            _: AVCaptureOutput,
            didOutput sampleBuffer: CMSampleBuffer,
            from _: AVCaptureConnection
        ) {
            guard !didScan,
                let pixelBuffer = CMSampleBufferGetImageBuffer(sampleBuffer)
            else { return }

            let code: String?
            do {
                code = try VisionQRCodeDecoder.payload(in: pixelBuffer)
            }
            catch {
                if !didLogDecodeFailure {
                    didLogDecodeFailure = true
                    logger.error(
                        "QR frame decoding failed: \(error.localizedDescription)"
                    )
                }
                return
            }
            guard let code else { return }
            didScan = true
            Task { @MainActor in
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
