import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("S3KeySheet")

/// Replacing the S3 key of the library on the Sync page: whether the S3 key
/// sheet is open and what it shows, the call that checks the key with S3
/// storage and uses it, and the page's line after a call: a save that names
/// the old key to delete, or a failure the sheet was no longer there to show.
/// The page owns it: Replace key… and Sign in to S3… both open the same
/// sheet. Nothing stores the line; it goes when the person dismisses it or
/// the page closes.
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

    /// The page's line after a call.
    enum Line: Equatable {
        /// bae saved the key. `oldKey` is the key the library recorded
        /// before, which the person deletes in the provider's console; nil
        /// when the library already recorded the new key.
        case saved(oldKey: String?)
        /// Using the key failed once the sheet no longer waited on the call:
        /// the person cancelled after the check had passed, or saved another
        /// key meanwhile. The library may still have recorded the key.
        case failed(DisplayError)
    }

    /// The open sheet's face; nil while the sheet is closed.
    private(set) var sheet: Sheet?
    /// The page's line after the last call, until the person dismisses it.
    private(set) var line: Line?

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
        // periphery:ignore - called by baeShots
        /// A replacement showing `sheet` and `line`, with no call behind it,
        /// for a shot.
        init(sheet: Sheet? = nil, line: Line? = nil) {
            self.makeCancellation = {
                preconditionFailure("a shot's key sheet makes no call")
            }
            self.sheet = sheet
            self.line = line
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
    /// it had already passed: then the call finishes and the page shows how
    /// it ended.
    func cancel() {
        if sheet == .checking {
            running?.cancellation.cancel()
        }
        sheet = nil
    }

    func dismissLine() {
        line = nil
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
        // Only the call the open sheet waits on changes the sheet. A call the
        // sheet no longer waits on tells the page how it ended instead.
        let shown = current && sheet == .checking
        switch ended {
        case .saved(let oldKey):
            line = .saved(oldKey: oldKey)
            if shown { sheet = nil }
        case .refused:
            if shown {
                sheet = .fields(.refused)
            }
            else {
                // A refused key changed nothing, so the page has nothing to
                // tell.
                logger.info(
                    "S3 storage refused a key whose sheet had closed"
                )
            }
        case .failed(let error):
            if shown {
                sheet = .fields(error.map(Failure.failed))
            }
            else {
                logger.error(
                    "Using a new S3 key failed after its sheet closed: \(error?.line ?? "a cancellation"), \(error?.detail ?? "no detail")"
                )
                // A failure with no line is a cancellation, which core says
                // has nothing to show.
                if let error { line = .failed(error) }
            }
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

    init(replacement: S3KeyReplacement) {
        self.replacement = replacement
    }

    #if DEBUG
        // periphery:ignore - called by baeShots
        /// The sheet with `accessKey` and `secretKey` already typed, for a
        /// shot.
        init(
            replacement: S3KeyReplacement,
            accessKey: String,
            secretKey: String
        ) {
            self.replacement = replacement
            _accessKey = State(initialValue: accessKey)
            _secretKey = State(initialValue: secretKey)
        }
    #endif

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
                MacS3KeyFields(accessKey: $accessKey, secretKey: $secretKey) {
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
                    MacButton("Save", kind: .primary) {
                        replacement.save(
                            accessKey: accessKey,
                            secretKey: secretKey,
                            sync: sync
                        )
                    }
                    .keyboardShortcut(.defaultAction)
                    .disabled(accessKey.isEmpty || secretKey.isEmpty)
                }
            }
        }
    }

    private func failureNotice(_ failure: S3KeyReplacement.Failure) -> MacNotice
    {
        let title = String(localized: "Could not use this key")
        switch failure {
        case .refused:
            return MacNotice(
                title: title,
                advice: String(
                    localized: "S3 storage does not accept this access key."
                ),
                error: nil,
                look: .quiet
            )
        case .failed(let error):
            return MacNotice(
                title: title,
                advice: error.line,
                error: error,
                look: .quiet
            )
        }
    }
}
