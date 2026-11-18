import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("S3KeySheet")

/// Replacing the S3 key of the library on the Sync page: whether the S3 key
/// sheet is open and what it shows, the call that checks the key with S3
/// storage and uses it, and the line after a save that names the old key to
/// delete. The page owns it: Replace key…, Sign in to S3… and a pending
/// record's S3 sign-in all open the same sheet. Nothing stores the old key's
/// id; it goes when the person dismisses the line or the page closes.
@MainActor
@Observable
final class S3KeyReplacement {
    /// What the open sheet shows.
    enum Sheet: Equatable {
        /// The two fields, with why the last key failed when it did.
        case fields(Failure?)
        /// bae checks the key with S3 storage; only Cancel shows.
        case checking
    }

    /// Why the last key changed nothing.
    enum Failure: Equatable {
        /// S3 storage refused the key.
        case refused
        /// The check could not finish, or another failure, in its own line.
        case failed(DisplayError)
    }

    /// The line after a save.
    struct Saved: Equatable {
        /// The key the library recorded before, which the person deletes in
        /// the provider's console; nil when the library already recorded
        /// the new key.
        let oldKey: String?
    }

    /// The open sheet's face; nil while the sheet is closed.
    private(set) var sheet: Sheet?
    /// The line after the last save, until the person dismisses it.
    private(set) var saved: Saved?

    /// The running call and its cancel. A call the person cancelled keeps
    /// running until its check stops, and its save still shows the line.
    @ObservationIgnored
    private var running: (id: UUID, cancellation: BridgeCancellation)?
    private let makeCancellation: () -> BridgeCancellation

    init(
        makeCancellation: @escaping () -> BridgeCancellation = {
            BridgeCancellation()
        }
    ) {
        self.makeCancellation = makeCancellation
    }

    #if DEBUG
        /// A replacement showing the line after `saved`, with no call
        /// behind it, for a shot.
        init(saved: Saved) {
            self.makeCancellation = {
                preconditionFailure("a shot's key sheet makes no call")
            }
            self.saved = saved
        }
    #endif

    var isOpen: Bool { sheet != nil }

    func open() {
        sheet = .fields(nil)
    }

    /// Check the key with S3 storage and use it in place of the library's.
    func save(accessKey: String, secretKey: String, sync: Sync) {
        let id = UUID()
        let cancellation = makeCancellation()
        running = (id, cancellation)
        sheet = .checking
        Task {
            do {
                let outcome = try await sync.useAccessKey(
                    accessKey,
                    secretKey,
                    cancellation
                )
                finish(id, Ended(outcome))
            }
            catch {
                logger.error(
                    "Using the new S3 key failed: \(String(reflecting: error))"
                )
                finish(id, .failed(DisplayError(error)))
            }
        }
    }

    /// Close the sheet. A check that runs stops, and nothing changes, unless
    /// it had already passed: then the save finishes and shows its line.
    func cancel() {
        if sheet == .checking {
            running?.cancellation.cancel()
        }
        sheet = nil
    }

    func dismissSaved() {
        saved = nil
    }

    /// How a call ended, as the sheet tells it.
    private enum Ended {
        case saved(oldKey: String?)
        case refused
        /// A failure with its line, or none for a failure core says has none
        /// to show.
        case failed(DisplayError?)
        case cancelled

        init(_ outcome: BridgeAccessKeyOutcome) {
            switch outcome {
            case .saved(let replaced): self = .saved(oldKey: replaced)
            case .refused: self = .refused
            case .failed(let error): self = .failed(DisplayError(error))
            case .cancelled: self = .cancelled
            }
        }
    }

    private func finish(_ id: UUID, _ ended: Ended) {
        let current = running?.id == id
        if current {
            running = nil
        }
        // Only the call the open sheet waits on changes the sheet.
        let shown = current && sheet == .checking
        switch ended {
        case .saved(let oldKey):
            saved = Saved(oldKey: oldKey)
            if shown { sheet = nil }
        case .refused:
            if shown { sheet = .fields(.refused) }
        case .failed(let error):
            if shown { sheet = .fields(error.map(Failure.failed)) }
        case .cancelled:
            break
        }
    }
}

/// The S3 key sheet: a new key for the library's S3 storage, which bae
/// checks with the storage before it uses it. The typed key stays while
/// bae checks it, so a refused key shows the fields again as typed.
struct S3KeySheet: View {
    let replacement: S3KeyReplacement

    @Environment(Sync.self)
    private var sync

    @State
    private var accessKey = ""
    @State
    private var secretKey = ""

    var body: some View {
        MacSheet(
            "S3 key",
            message: "Enter a key that can use this library’s S3 storage.",
            width: 440
        ) {
            if replacement.sheet == .checking {
                MacProgress(
                    title: String(localized: "Checking the key…"),
                    bar: ProgressTrackBar(progress: nil)
                )
            }
            else {
                S3KeyFields(accessKey: $accessKey, secretKey: $secretKey) {
                    if case .fields(let failure?) = replacement.sheet {
                        failureNotice(failure)
                    }
                }
            }
        } footer: {
            MacSheetFooter {
                Button("Cancel") { replacement.cancel() }
                    .keyboardShortcut(.cancelAction)
                if replacement.sheet != .checking {
                    Button("Save") {
                        replacement.save(
                            accessKey: accessKey,
                            secretKey: secretKey,
                            sync: sync
                        )
                    }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(accessKey.isEmpty || secretKey.isEmpty)
                }
            }
        }
    }

    private func failureNotice(_ failure: S3KeyReplacement.Failure) -> Notice {
        let title = String(localized: "Could not use this key")
        switch failure {
        case .refused:
            return Notice(
                title: title,
                advice: String(
                    localized: "S3 storage does not accept this access key."
                ),
                error: nil,
                look: .quiet
            )
        case .failed(let error):
            return Notice(
                title: title,
                advice: error.line,
                error: error,
                look: .quiet
            )
        }
    }
}
