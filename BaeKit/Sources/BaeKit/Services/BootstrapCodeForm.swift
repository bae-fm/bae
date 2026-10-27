import Foundation
import Observation
import os.log

private let logger = Logger.bae("BootstrapCodeForm")

/// What a code screen's primary action runs: a restore from a saved or entered
/// restore code, or a join by invitation.
public enum BootstrapAttempt {
    case restoreSaved(SavedRestoreCode)
    /// A restore from a typed or scanned code. A code the person's own
    /// device showed holds their S3 key, as `metadata` says.
    case restore(code: String, metadata: BridgeCodeMetadata)
    /// A join from an invite's code, which starts as soon as the code reads:
    /// the invite carries the person's name.
    case join(code: String, invite: BridgeInviteInfo)
    /// A join this device has under way, continued after bae restarted.
    case resume(BridgeUnfinishedJoin, invite: BridgeInviteInfo)

    /// Whether this restore needs the person's S3 key typed in: an S3 library
    /// whose key isn't saved beside the code and isn't in the code.
    public var needsS3Key: Bool {
        switch self {
        case .restoreSaved(let saved):
            if case .s3 = saved.info.location {
                return !saved.info.hasS3Credentials
            }
            return false
        case .restore(_, let metadata):
            return metadata.provider == .s3 && !metadata.carriesS3Key
        case .join, .resume:
            return false
        }
    }

    /// The invite a join runs from; nil for a restore.
    public var invite: BridgeInviteInfo? {
        switch self {
        case .join(_, let invite), .resume(_, let invite): invite
        case .restoreSaved, .restore: nil
        }
    }
}

/// One restore or join screen's state: the code typed, pasted or scanned (or
/// the saved one it opened with), what that code decodes to, the S3 key a
/// restore needs, and the running attempt. An invite joins as soon as its code
/// reads. The apps' code screens render it and call its actions.
@MainActor
@Observable
public final class BootstrapCodeForm {
    public enum Entry {
        /// A restore, from a saved code or one the person enters.
        case restore(BridgeSavedRestoreCode?)
        case join
        /// A join this device has under way when bae starts, continued at
        /// once.
        case resume(BridgeUnfinishedJoin)
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
    private let unfinishedJoins:
        @Sendable () async throws -> [BridgeUnfinishedJoin]
    @ObservationIgnored
    private let onLibraryReady: @MainActor (OpenedLibrary, BridgeError?) -> Void

    /// `onLibraryReady` receives the restored or joined library's open store,
    /// with the error a partly finished restore or join reported.
    public init(
        entry: Entry,
        makeBootstrap: @escaping @MainActor () -> LibraryBootstrap,
        unfinishedJoins:
            @escaping @Sendable () async throws -> [BridgeUnfinishedJoin],
        onLibraryReady:
            @escaping @MainActor (OpenedLibrary, BridgeError?) -> Void
    ) {
        self.entry = entry
        self.makeBootstrap = makeBootstrap
        self.unfinishedJoins = unfinishedJoins
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
        if case .resume(let join) = entry {
            return .resume(join, invite: join.info())
        }
        guard case .success(let info) = decoded else { return nil }
        let code = codeInput.trimmingCharacters(in: .whitespacesAndNewlines)
        return switch (info, entry) {
        case (.restore(let metadata), _):
            .restore(code: code, metadata: metadata)
        // Restoring over this device's copy takes no invite; core says so.
        case (.invite(let invite), .restoreOver),
            (.invite(let invite), .finishRestore):
            .restore(code: code, metadata: invite.metadata)
        case (.invite(let invite), _):
            .join(code: code, invite: invite)
        }
    }

    /// Whether Cancel can stop the running attempt: a restore until it
    /// ends, a join only until its request may have been sent, which coven
    /// reports as waiting for approval.
    public var cancellable: Bool {
        guard isRunning else { return false }
        if attempt?.invite == nil { return true }
        switch status {
        case nil, .connecting: return true
        case .waitingForApproval, .loading, .pending: return false
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
        return true
    }

    /// Fill the code from a scanned QR code. A code from the person's own
    /// device holds their S3 key, so the restore asks for none.
    public func acceptScan(_ text: String) {
        codeInput = text.trimmingCharacters(in: .whitespacesAndNewlines)
        decodeCode()
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
        // An invite carries everything joining needs, so it joins at once.
        if attempt?.invite != nil, !isRunning { start() }
    }

    /// Start a join this screen opened to resume; the screen calls it as it
    /// appears.
    public func begin() {
        if case .resume = entry, !isRunning { start() }
    }

    public func start() {
        guard let attempt, !isRunning else {
            preconditionFailure("the primary action needs a decoded code")
        }
        let credentials: BridgeS3Credentials? =
            attempt.needsS3Key
            ? BridgeS3Credentials(
                accessKeyId: accessKey,
                secretAccessKey: secretKey
            ) : nil
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
                    credentials: credentials
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
        credentials: BridgeS3Credentials?
    ) async throws -> BridgeBootstrapResult {
        switch (attempt, entry) {
        case (.restoreSaved(let saved), _):
            try await bootstrap.restore(
                saved: saved.code,
                credentials: credentials
            )
        case (.restore(let code, _), .restoreOver(let libraryId)),
            (.restore(let code, _), .finishRestore(let libraryId)):
            try await bootstrap.restoreReplacing(
                libraryId: libraryId,
                code: code,
                credentials: credentials
            )
        case (.restore(let code, _), _):
            try await bootstrap.restore(code: code, credentials: credentials)
        case (.join(let code, _), _):
            try await bootstrap.join(code: code)
        case (.resume(let join, _), _):
            try await bootstrap.resume(join)
        }
    }

    /// Stop the running attempt while `cancellable`; its screen stays as it
    /// was.
    public func cancel() {
        if attempt?.invite != nil {
            bootstrap?.cancel()
        }
        else {
            task?.cancel()
        }
    }

    /// The screen went away: a restore stops, and a join's call is dropped,
    /// which keeps the join for bae's next start.
    public func leave() {
        task?.cancel()
    }

    /// End the running join's wait so it checks for approval now.
    public func checkNow() {
        bootstrap?.checkNow()
    }

    /// Continue the join this device has under way, after a cancel came as
    /// its request was sent.
    private func resumeUnfinished() {
        let list = unfinishedJoins
        isRunning = true
        task = Task {
            defer {
                task = nil
                isRunning = false
            }
            do {
                guard let join = try await list().first else { return }
                let bootstrap = makeBootstrap()
                self.bootstrap = bootstrap
                deliver(try await bootstrap.resume(join))
            }
            catch is CancellationError {
                logger.debug("resumed join cancelled")
            }
            catch {
                logger.error(
                    "resuming the join failed: \(error.localizedDescription)"
                )
                self.error = DisplayError(error)
            }
        }
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
                        "The invite was declined, cancelled or has expired."
                )
            )
        case .joinRequestSent:
            // After the running attempt's task has ended.
            Task { resumeUnfinished() }
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
