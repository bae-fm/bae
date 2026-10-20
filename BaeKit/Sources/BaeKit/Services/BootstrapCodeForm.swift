import Foundation
import Observation
import os.log

private let logger = Logger.bae("BootstrapCodeForm")

/// What a code screen's primary action runs: a restore from a saved or entered
/// restore code, or a join by invitation.
public enum BootstrapAttempt {
    case restoreSaved(SavedRestoreCode)
    /// A restore from a typed or scanned code; `s3Key` is the key that came
    /// beside a scanned code from the person's own device.
    case restore(
        code: String,
        metadata: BridgeCodeMetadata,
        s3Key: BridgeS3Credentials?
    )
    case join(code: String, metadata: BridgeCodeMetadata, libraryName: String)

    /// Whether this restore needs the person's S3 key typed in: an S3 library
    /// whose key isn't saved beside the code and didn't come with the scan.
    public var needsS3Key: Bool {
        switch self {
        case .restoreSaved(let saved):
            if case .s3 = saved.info.location {
                return !saved.info.hasS3Credentials
            }
            return false
        case .restore(_, let metadata, let s3Key):
            return metadata.provider == .s3 && s3Key == nil
        case .join:
            return false
        }
    }
}

/// One restore or join screen's state: the code typed, pasted or scanned (or
/// the saved one it opened with), what that code decodes to, what the person
/// adds to it (their name for a join, the S3 key a restore needs), and the
/// running attempt. The apps' code screens render it and call its actions.
@MainActor
@Observable
public final class BootstrapCodeForm {
    public enum Entry {
        /// A restore, from a saved code or one the person enters.
        case restore(BridgeSavedRestoreCode?)
        case join
        /// A restore over this device's copy of a library, with the person's
        /// current restore code, after another of their devices replaced
        /// their keys. Core refuses any other code.
        case restoreOver(libraryId: String)
        /// The same restore again, to finish one that stopped after the
        /// restored copy took the place of this device's copy. Core refuses
        /// any other code.
        case finishRestore(libraryId: String)
    }

    public let entry: Entry
    /// The saved code a restore opened with, read once.
    public let saved: SavedRestoreCode?
    public var codeInput = ""
    /// The entered code's kind and metadata, or nil while the field is empty.
    private var decoded: Result<BridgeCodeInfo, Error>?
    /// The S3 key the last scan carried beside its code. It belongs to that
    /// code alone: editing the code away from it leaves the key unused.
    private var scannedKey: (code: String, key: BridgeS3Credentials)?
    public var personName = ""
    public var accessKey = ""
    public var secretKey = ""
    /// The running attempt, which reports its progress.
    private var bootstrap: LibraryBootstrap?
    public private(set) var isRunning = false
    public private(set) var error: DisplayError?

    @ObservationIgnored
    private var task: Task<Void, Never>?
    @ObservationIgnored
    private let makeBootstrap: @MainActor () -> LibraryBootstrap
    @ObservationIgnored
    private let onLibraryReady: @MainActor (OpenedLibrary, BridgeError?) -> Void

    /// `onLibraryReady` receives the restored or joined library's open store,
    /// with the error a partly finished restore or join reported.
    public init(
        entry: Entry,
        makeBootstrap: @escaping @MainActor () -> LibraryBootstrap,
        onLibraryReady:
            @escaping @MainActor (OpenedLibrary, BridgeError?) -> Void
    ) {
        self.entry = entry
        self.makeBootstrap = makeBootstrap
        self.onLibraryReady = onLibraryReady
        if case .restore(let code?) = entry {
            saved = SavedRestoreCode(code: code)
        }
        else {
            saved = nil
        }
    }

    /// What the running attempt is doing, nil once it finished.
    public var status: BridgeBootstrapStatus? { bootstrap?.status }

    public var attempt: BootstrapAttempt? {
        if let saved { return .restoreSaved(saved) }
        guard case .success(let info) = decoded else { return nil }
        let code = codeInput.trimmingCharacters(in: .whitespacesAndNewlines)
        let s3Key = scannedKey.flatMap { $0.code == code ? $0.key : nil }
        return switch (info, entry) {
        case (.restore(let metadata), _):
            .restore(code: code, metadata: metadata, s3Key: s3Key)
        // Restoring over this device's copy takes no invite; core says so.
        case (.invite(let metadata, _), .restoreOver),
            (.invite(let metadata, _), .finishRestore):
            .restore(code: code, metadata: metadata, s3Key: s3Key)
        case (.invite(let metadata, let libraryName), _):
            .join(code: code, metadata: metadata, libraryName: libraryName)
        }
    }

    /// Why the entered code doesn't decode, or nil.
    public var decodeError: DisplayError? {
        guard case .failure(let error) = decoded else { return nil }
        return DisplayError(error)
    }

    public var ready: Bool {
        guard let attempt, !isRunning else { return false }
        if attempt.needsS3Key && (accessKey.isEmpty || secretKey.isEmpty) {
            return false
        }
        if case .join = attempt {
            return !personName.trimmingCharacters(in: .whitespaces).isEmpty
        }
        return true
    }

    /// Fill the code from a scanned QR code. A key that came beside it from
    /// the person's own device fills in the S3 key, so the restore asks for
    /// none.
    public func acceptScan(_ text: String) {
        do {
            let scanned = try readScannedCode(text: text)
            scannedKey = scanned.s3Credentials.map { (scanned.code, $0) }
            codeInput = scanned.code
            decodeCode()
        }
        catch {
            logger.error(
                "scanned code unreadable: \(error.localizedDescription)"
            )
            scannedKey = nil
            decoded = .failure(error)
        }
    }

    /// Read what the entered code is. Called whenever the code changes.
    public func decodeCode() {
        error = nil
        let code = codeInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty else {
            decoded = nil
            return
        }
        decoded = Result { try decodeCodeInfo(code: code) }
    }

    public func start() {
        guard let attempt, !isRunning else {
            preconditionFailure("the primary action needs a decoded code")
        }
        let credentials: BridgeS3Credentials? =
            switch attempt {
            case .restore(_, _, let s3Key?): s3Key
            default:
                attempt.needsS3Key
                    ? BridgeS3Credentials(
                        accessKeyId: accessKey,
                        secretAccessKey: secretKey
                    ) : nil
            }
        let personName = personName.trimmingCharacters(in: .whitespaces)
        let bootstrap = makeBootstrap()
        self.bootstrap = bootstrap
        error = nil
        isRunning = true
        task = Task {
            defer {
                task = nil
                isRunning = false
            }
            do {
                let result = try await run(
                    attempt,
                    on: bootstrap,
                    credentials: credentials,
                    personName: personName
                )
                // A result that holds an open store is delivered even after a
                // cancel raced it, so the store is opened rather than leaked.
                deliver(result)
            }
            catch is CancellationError {
                logger.debug("bootstrap cancelled")
            }
            catch {
                logger.error("bootstrap failed: \(error.localizedDescription)")
                self.error = DisplayError(error)
            }
        }
    }

    /// Run `attempt` on `bootstrap`: a restore from a saved or entered code,
    /// over this device's copy when the entry says so, or a join.
    private func run(
        _ attempt: BootstrapAttempt,
        on bootstrap: LibraryBootstrap,
        credentials: BridgeS3Credentials?,
        personName: String
    ) async throws -> BridgeBootstrapResult {
        switch (attempt, entry) {
        case (.restoreSaved(let saved), _):
            try await bootstrap.restore(
                saved: saved.code,
                credentials: credentials
            )
        case (.restore(let code, _, _), .restoreOver(let libraryId)),
            (.restore(let code, _, _), .finishRestore(let libraryId)):
            try await bootstrap.restoreReplacing(
                libraryId: libraryId,
                code: code,
                credentials: credentials
            )
        case (.restore(let code, _, _), _):
            try await bootstrap.restore(code: code, credentials: credentials)
        case (.join(let code, _, _), _):
            try await bootstrap.join(code: code, personName: personName)
        }
    }

    /// Stop the running attempt; its screen stays as it was.
    public func cancel() {
        task?.cancel()
    }

    private func deliver(_ result: BridgeBootstrapResult) {
        switch result {
        case .opened(let library):
            onLibraryReady(library, nil)
        case .published(let library, let warning),
            .profileRequired(let library, let warning):
            onLibraryReady(library, warning)
        case .declined:
            error = DisplayError(
                line: String(
                    localized:
                        "An admin declined this device's request to join."
                )
            )
        case .savedCodeRemoved:
            error = DisplayError(
                line: String(
                    localized:
                        "This recovery code is no longer saved in iCloud Keychain."
                )
            )
        }
    }
}
