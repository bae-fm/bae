import AVFoundation
import AuthenticationServices
import SwiftUI
import UIKit
import os.log

private let logger = Logger.bae("OnboardingView")

@MainActor
private final class LinkFlow {
    var bridgeOperation: RestoreFromCodeOperation?
    private let makeTask: (LinkFlow) -> Task<Void, Never>
    private lazy var task: Task<Void, Never> = makeTask(self)

    init(makeTask: @escaping (LinkFlow) -> Task<Void, Never>) {
        self.makeTask = makeTask
    }

    func start() {
        _ = task
    }

    func cancel() {
        bridgeOperation?.cancel()
        task.cancel()
    }
}

/// First-run linking. Scan a QR code or paste a restore code from bae desktop
/// to connect this device to your library. On submit: decode the code, run
/// cloud sign-in when the provider needs it, inject the CloudKit driver when the
/// library syncs through CloudKit, then restore.
struct OnboardingView: View {
    // The host's OAuth client config. Present only in a full build; baeium
    // (S3-only) compiles out the OAuth branch of the link flow.
    #if BAE_OAUTH_PROVIDERS
    let oauthLinking: OAuthLinking?
    let oauthLinkingError: String?
    #endif
    let onLinked: (BridgeLibrary) -> Void

    @State
    private var showScanner = false
    @State
    private var showPasteSheet = false
    @State
    private var pasteInput = ""
    @State
    private var error: String?
    @State
    private var linkFlow: LinkFlow?
    #if BAE_OAUTH_PROVIDERS
    @State
    private var presentationAnchor: ASPresentationAnchor?
    #endif

    var body: some View {
        Group {
            if linkFlow != nil {
                linkingView
            }
            else {
                entryView
            }
        }
        .fullScreenCover(isPresented: $showScanner) {
            scannerSheet
        }
        .sheet(isPresented: $showPasteSheet) {
            pasteSheet
        }
        .onDisappear {
            linkFlow?.cancel()
        }
        #if BAE_OAUTH_PROVIDERS
        // Captures the host window so the OAuth web-auth session has a
        // presentation anchor. Only the OAuth link branch needs it.
        .background(
            PresentationAnchorReader(presentationAnchor: $presentationAnchor)
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
        )
        #endif
    }

    private var entryView: some View {
        onboardingScreen {
            Image(systemName: "music.note.house.fill")
                .font(.system(size: 72))
                .foregroundStyle(Theme.accent)
            Text("bae")
                .font(.system(size: 48, weight: .bold))
            secondaryText("Scan a QR or paste a code from your library to get started")

            VStack(spacing: 12) {
                Button {
                    error = nil
                    requestCameraThenScan()
                } label: {
                    Text("Scan QR")
                        .frame(maxWidth: 240)
                }
                .buttonStyle(.borderedProminent)

                Button {
                    error = nil
                    pasteInput = ""
                    showPasteSheet = true
                } label: {
                    Text("Paste code")
                        .frame(maxWidth: 240)
                }
                .buttonStyle(.bordered)
            }
            .padding(.top, 16)

            if let error {
                Text(error)
                    .font(.callout)
                    .foregroundStyle(.red)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 320)
            }
        }
    }

    private var linkingView: some View {
        onboardingScreen {
            ProgressView()
                .controlSize(.large)
            Text("Connecting to your library")
                .font(.headline)
                .multilineTextAlignment(.center)
            secondaryText("bae is restoring the library on this device.")
            Button("Cancel") {
                cancelLink()
            }
            .buttonStyle(.bordered)
            .padding(.top, 8)
        }
    }

    private func onboardingScreen<Content: View>(
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(spacing: 16) {
            Spacer()
            content()
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(32)
    }

    private func secondaryText(_ text: LocalizedStringKey) -> some View {
        Text(text)
            .font(.body)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .frame(maxWidth: 320)
    }

    private var scannerSheet: some View {
        ZStack(alignment: .topTrailing) {
            QRScannerView(
                onScanned: { code in
                    showScanner = false
                    link(code: code)
                },
                onError: { message in
                    showScanner = false
                    error = message
                }
            )
            .ignoresSafeArea()
            Button {
                showScanner = false
            } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(.title)
                    .foregroundStyle(.white)
                    .padding()
            }
        }
    }

    private var pasteSheet: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 16) {
                Text(
                    "Paste the code from bae desktop \u{2192} Settings \u{2192} Library \u{2192} Connect another device."
                )
                .font(.callout)
                .foregroundStyle(.secondary)
                TextField("Paste your restore code", text: $pasteInput, axis: .vertical)
                    .textFieldStyle(.roundedBorder)
                    .font(.body.monospaced())
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .lineLimit(3, reservesSpace: true)
                Spacer()
            }
            .padding()
            .navigationTitle("Paste code")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { showPasteSheet = false }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Connect") {
                        let code = pasteInput.trimmingCharacters(
                            in: .whitespacesAndNewlines
                        )
                        showPasteSheet = false
                        link(code: code)
                    }
                    .disabled(
                        pasteInput.trimmingCharacters(
                            in: .whitespacesAndNewlines
                        )
                        .isEmpty
                    )
                }
            }
        }
    }

    private func requestCameraThenScan() {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized:
            showScanner = true
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { granted in
                DispatchQueue.main.async {
                    if granted {
                        showScanner = true
                    }
                    else {
                        error = String(
                            localized:
                                "Camera permission is required to scan QR codes"
                        )
                    }
                }
            }
        default:
            error = String(
                localized:
                    "Camera access is denied. Enable it in Settings to scan QR codes."
            )
        }
    }

}

extension OnboardingView {
    func cancelLink() {
        linkFlow?.cancel()
        linkFlow = nil
    }

    /// Decode the restore code, run cloud sign-in when required, inject the
    /// CloudKit driver before restore when the library syncs through CloudKit,
    /// and restore. A restore code points at the owner's own private CloudKit
    /// zone — every device is the one owner.
    func link(code: String) {
        error = nil
        cancelLink()
        let flow = LinkFlow { flow in
            Task {
                defer {
                    if linkFlow === flow {
                        linkFlow = nil
                    }
                }
                await performLink(code: code, flow: flow)
            }
        }
        linkFlow = flow
        flow.start()
    }

    /// The link work, off the `LinkFlow`'s task: decode, obtain a cloud token
    /// when the provider needs one, then restore. Sets `error` and returns on a
    /// handled failure; logs and ignores cancellation.
    private func performLink(code: String, flow: LinkFlow) async {
        do {
            let info = try decodeRestoreCode(code: code)
            let tokenJson: String?
            if info.needsOauth {
                guard let token = try await oauthToken(for: info) else {
                    return
                }
                tokenJson = token
            }
            else {
                tokenJson = nil
            }
            try await restore(code: code, tokenJson: tokenJson, flow: flow)
        }
        catch {
            if isLinkCancellation(error) {
                logger.debug("link flow cancelled")
            }
            else {
                self.error = error.localizedDescription
            }
        }
    }

    /// The cloud token for a library that needs OAuth, or `nil` after setting
    /// `error` when this build / config can't satisfy it. Only called when
    /// `info.needsOauth`: a provider like Google Drive runs the system auth
    /// session; CloudKit and S3 need none.
    private func oauthToken(for info: BridgeRestoreCodeInfo) async throws -> String? {
        #if BAE_OAUTH_PROVIDERS
        if let oauthLinkingError {
            error = oauthLinkingError
            return nil
        }
        guard let linking = oauthLinking else {
            error = String(
                localized:
                    "This library needs cloud sign-in, which isn't configured on this build."
            )
            return nil
        }
        guard let presentationAnchor else {
            throw OAuthLinkingError.noPresentationAnchor
        }
        return try await linking.authorize(
            provider: info.cloudProvider,
            presentationAnchor: presentationAnchor
        )
        #else
        // A baeium (S3-only) build can't sign in to OAuth providers at all, so a
        // library that needs one can't be linked here.
        error = String(
            localized:
                "This library syncs through a cloud provider this build doesn't support."
        )
        return nil
        #endif
    }

    private func restore(
        code: String,
        tokenJson: String?,
        flow: LinkFlow
    ) async throws {
        let bridgeOperation = try restoreFromCodeOperation(
            code: code,
            oauthTokenJson: tokenJson
        )
        flow.bridgeOperation = bridgeOperation
        let libraryInfo = try await withTaskCancellationHandler {
            try Task.checkCancellation()
            return
                try await Task.detached {
                    try bridgeOperation.restore()
                }
                .value
        } onCancel: {
            bridgeOperation.cancel()
        }
        try Task.checkCancellation()
        onLinked(libraryInfo)
    }

    private func isLinkCancellation(_ error: Error) -> Bool {
        if error is CancellationError {
            return true
        }
        if case BridgeError.Cancelled = error {
            return true
        }
        return false
    }
}

#if BAE_OAUTH_PROVIDERS
private struct PresentationAnchorReader: UIViewRepresentable {
    @Binding
    var presentationAnchor: ASPresentationAnchor?

    func makeUIView(context: Context) -> UIView {
        UIView(frame: .zero)
    }

    func updateUIView(_ uiView: UIView, context: Context) {
        let window = uiView.window
        DispatchQueue.main.async {
            presentationAnchor = window
        }
    }
}
#endif
