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
                placeholder(String(localized: "Starting camera..."))
            case .ready(let capture):
                CameraPreview(capture: capture, onScan: onScan)
            case .denied:
                placeholder(
                    String(
                        localized:
                            "Camera access is off. Enable it in System Settings, or paste the code below."
                    )
                )
            case .unavailable:
                placeholder(
                    String(
                        localized:
                            "No camera available. Paste the code below instead."
                    )
                )
            }
        }
        .task { await start() }
        .onDisappear { stop() }
    }

    private func placeholder(_ message: String) -> some View {
        Text(message)
            .themeText(.body)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
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
                "Failed to open camera input: \(String(reflecting: error))"
            )
            state = .unavailable
            return
        }
        // A phone shows a join or invite code up to QR version 20, which
        // Vision reads from 20 to 25 cm in 1920x1080 frames but only from
        // 15 cm in 1280x720 ones.
        if session.canSetSessionPreset(.hd1920x1080) {
            session.sessionPreset = .hd1920x1080
        }
        logger.info(
            "QR camera \(device.localizedName): preset \(session.sessionPreset.rawValue)"
        )
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
struct CodeScannerSheet: View {
    let onScan: (String) -> Void
    let onDismiss: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Scan a code")
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
        private var tally = FrameTally()

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

            tally.start(pixelBuffer)
            let code: String?
            do {
                code = try VisionQRCodeDecoder.payload(in: pixelBuffer)
            }
            catch {
                tally.record(read: false)
                if !didLogDecodeFailure {
                    didLogDecodeFailure = true
                    logger.error(
                        "QR frame decoding failed: \(String(reflecting: error))"
                    )
                }
                return
            }
            tally.record(read: code != nil)
            guard let code else { return }
            didScan = true
            Task { @MainActor in
                self.onScan(code)
            }
        }
    }

    /// Logs the camera's frames: their size and format once, when the first
    /// arrives, and how many were read once a second after that, so a scan
    /// that never reads a code shows what Vision was given. Runs on the scan
    /// queue only.
    struct FrameTally {
        private var started = false
        private var windowStart = ContinuousClock.now
        private var analysed = 0
        private var read = 0

        mutating func start(_ pixelBuffer: CVPixelBuffer) {
            guard !started else { return }
            started = true
            windowStart = .now
            let format = CVPixelBufferGetPixelFormatType(pixelBuffer)
            logger.info(
                "QR frames: \(CVPixelBufferGetWidth(pixelBuffer))x\(CVPixelBufferGetHeight(pixelBuffer)), pixel format \(format)"
            )
        }

        mutating func record(read didRead: Bool) {
            analysed += 1
            if didRead { read += 1 }
            let elapsed = windowStart.duration(to: .now)
            guard didRead || elapsed >= .seconds(1) else { return }
            logger.info(
                "QR frames in \(elapsed.formatted(.units(allowed: [.milliseconds]))): \(analysed) analysed, \(read) read"
            )
            windowStart = .now
            analysed = 0
            read = 0
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
