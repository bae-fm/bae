import AVFoundation
import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("QRScanner")

/// A live camera QR-code scanner that calls `onScan` with the first decoded
/// code, or explains why the camera isn't available.
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
                .themeText(.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color.secondary.opacity(ThemeOpacity.tint))
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

/// Owns the capture session and runs its blocking start/stop calls in order,
/// so closing the scanner can't leave the camera running.
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

/// A sheet hosting `QRScannerView` over a screen whose paste field stays the
/// fallback.
struct PairingScannerSheet: View {
    let onScan: (String) -> Void
    let onDismiss: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Pairing code")
                    .themeText(.heading)
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

/// Shows the camera and forwards the first decoded QR code.
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
        /// Set after the first decode so a held-up code fires only once.
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

    /// A view backed by the capture preview layer.
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
