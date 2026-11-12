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
    /// A join from an invite's code. It starts once the person has typed
    /// their name, which the join's request carries.
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

/// Where an invite's code goes once it reads.
enum InviteRouting {
    /// No invite code has read.
    case unread
    /// Looking for a join this device has under way for its library.
    case checking
    /// This device has a join under way for the invite's library: it
    /// goes on with that join, which asks no name (coven refuses a
    /// second, different one).
    case resumes(BridgeUnfinishedJoin)
    /// No join is under way: the person gives their name first.
    case asksName
}

/// One restore or join screen's state: the code typed, pasted or scanned (or
/// the saved one it opened with), what that code decodes to, the S3 key a
/// restore needs, and the running attempt. An invite asks the person's name
/// before it joins, unless a join for its library is under way here; a scanned
/// restore code starts as soon as it holds all it needs, and a typed one waits
/// for the primary action. The apps' code screens render it
/// and call its actions.
@MainActor
@Observable
public final class BootstrapCodeForm {
    public enum Entry {
        /// A restore from a code saved in iCloud Keychain.
        case restoreSaved(BridgeSavedRestoreCode)
        /// A code the person types, pastes or scans: an invite joins, and a
        /// restore code adds this device.
        case code
        /// A join this device has under way when bae starts, continued at
        /// once.
        case resume(BridgeUnfinishedJoin)
        /// A restore over this device's copy of a library, with the person's
        /// current restore code, after another of their devices replaced
        /// their keys. Core refuses any other code.
        case restoreOver(libraryId: String)
        /// Finishing a restore of this device's copy that stopped partway,
        /// which begins at once from what core kept, with no code. The
        /// person's current restore code finishes it too, signing in again
        /// when the kept sign-in is refused. Core refuses any other code.
        case finishRestore(libraryId: String)
        /// A restore onto this device that stopped after it recorded itself,
        /// finished at once from what core kept, with no code. The person's
        /// restore code finishes it too.
        case resumeRestore(BridgeUnfinishedRestore)
    }

    public let entry: Entry
    /// The saved code a restore opened with, read once.
    public let saved: SavedRestoreCode?
    /// The code as the field shows it, in groups of four.
    public private(set) var codeInput = ""
    /// The entered code's kind and metadata, or nil while the field is empty.
    private var decoded: Result<BridgeCodeInfo, Error>?
    public var accessKey = ""
    public var secretKey = ""
    /// The name the person types for a join, on the step that follows an
    /// invite's code.
    public var joinName = ""
    /// What reading an invite's code led to, which only a join needs.
    private var inviteRouting = InviteRouting.unread
    /// Whether the join's waiting screen shows, which each join's control is
    /// told as it starts and whenever this changes.
    public private(set) var waitingScreenShown = false
    /// The invite of a join that ended without letting this device in, until
    /// the person acknowledges it.
    public private(set) var ended: BridgeInviteInfo?
    /// The running attempt, which reports its progress.
    private var bootstrap: LibraryBootstrap?
    public private(set) var isRunning = false
    public private(set) var error: DisplayError?

    @ObservationIgnored
    private var task: Task<Void, Never>?
    @ObservationIgnored
    private var routing: Task<Void, Never>?
    @ObservationIgnored
    private let decoder: (String) throws -> BridgeCodeInfo
    @ObservationIgnored
    private let makeBootstrap: @MainActor () -> LibraryBootstrap
    @ObservationIgnored
    private let unfinishedJoins:
        @Sendable () async throws -> [BridgeUnfinishedJoin]
    /// The id of the library a join under way is for.
    @ObservationIgnored
    private let joinLibrary: (BridgeUnfinishedJoin) -> String
    @ObservationIgnored
    private let unfinishedRestores:
        @Sendable () async throws -> [BridgeUnfinishedRestore]
    @ObservationIgnored
    private let onLibraryReady: @MainActor (OpenedLibrary, BridgeError?) -> Void

    /// `decodeCode` reads what an entered code is. `onLibraryReady` receives
    /// the restored or joined library's open store, with the error a partly
    /// finished restore or join reported.
    public init(
        entry: Entry,
        decodeCode: @escaping (String) throws -> BridgeCodeInfo,
        makeBootstrap: @escaping @MainActor () -> LibraryBootstrap,
        unfinishedJoins:
            @escaping @Sendable () async throws -> [BridgeUnfinishedJoin],
        unfinishedRestores:
            @escaping @Sendable () async throws -> [BridgeUnfinishedRestore],
        joinLibrary:
            @escaping (BridgeUnfinishedJoin) -> String = {
                $0.info().metadata.libraryId
            },
        onLibraryReady:
            @escaping @MainActor (OpenedLibrary, BridgeError?) -> Void
    ) {
        self.joinLibrary = joinLibrary
        self.entry = entry
        decoder = decodeCode
        self.makeBootstrap = makeBootstrap
        self.unfinishedJoins = unfinishedJoins
        self.unfinishedRestores = unfinishedRestores
        self.onLibraryReady = onLibraryReady
        if case .restoreSaved(let code) = entry {
            saved = SavedRestoreCode(code: code)
        }
        else {
            saved = nil
        }
    }

    /// What the running attempt is doing, nil once it finished.
    public var status: BridgeBootstrapStatus? { bootstrap?.status }

    /// The steps of the open the running attempt runs, if it opens this
    /// device's own copy.
    public var openSteps: LatestSource<BridgeOpenProgress>? {
        bootstrap?.openSteps
    }

    /// The counted step the running attempt runs, opening or loading.
    public var steps: LatestSource<BridgeBootstrapStep>? { bootstrap?.steps }

    public var attempt: BootstrapAttempt? {
        if let saved { return .restoreSaved(saved) }
        // The join under way, until the person enters a code of their own.
        if case .resume(let join) = entry, decoded == nil {
            return .resume(join, invite: join.info())
        }
        guard case .success(let info) = decoded else { return nil }
        let code = codeAsEntered(text: codeInput)
        return switch (info, entry) {
        case (.restore(let metadata), _):
            .restore(code: code, metadata: metadata)
        // Restoring over this device's copy takes no invite; core says so.
        case (.invite(let invite), .restoreOver),
            (.invite(let invite), .finishRestore):
            .restore(code: code, metadata: invite.metadata)
        case (.invite(let invite), _):
            if case .resumes(let join) = inviteRouting {
                .resume(join, invite: invite)
            }
            else {
                .join(code: code, invite: invite)
            }
        }
    }

    /// Whether Cancel can stop the running attempt: core decides, and says so
    /// with each state of the attempt.
    public var cancellable: Bool {
        guard isRunning, let bootstrap else { return false }
        return bootstrap.cancellable
    }

    /// Why the entered code doesn't decode, or nil.
    public var decodeError: DisplayError? {
        guard case .failure(let error) = decoded else { return nil }
        return DisplayError(error)
    }

    public var ready: Bool {
        guard let attempt, !isRunning else { return false }
        if case .join = attempt { return canJoin }
        if attempt.needsS3Key && (accessKey.isEmpty || secretKey.isEmpty) {
            return false
        }
        return true
    }

    /// Whether the primary action can run: the attempt has what it needs, or
    /// a code is typed that it would read first.
    public var canContinue: Bool {
        if attempt != nil { return ready }
        return !isRunning
            && !codeAsEntered(text: codeInput).isEmpty
    }

    /// The primary action of a screen that reads the typed code only when the
    /// person chooses it: start the attempt, or read the code first, which
    /// starts it unless it still needs the S3 key.
    public func submit() {
        if attempt != nil {
            start()
        }
        else {
            acceptCode(codeInput)
        }
    }

    /// Take `text`, a scanned QR code or a code the person chose Continue on,
    /// and go on at once: an invite joins, and a restore starts, unless it
    /// is an S3 code without the key, which waits for the key. A code from
    /// the person's own device holds their S3 key.
    public func acceptCode(_ text: String) {
        codeInput = codeInGroups(text: text)
        decodeCode()
        if ready { start() }
    }

    /// The person typed or pasted `text` into the code field: it shows in
    /// groups of four, and what the last reading made of the old text is
    /// dropped.
    public func edit(code text: String) {
        codeInput = codeInGroups(text: text)
        clearReading()
    }

    /// Forget what the entered code read as: the text changed, or the person
    /// went back from the S3 key to the code.
    public func clearReading() {
        decoded = nil
        error = nil
        routing?.cancel()
        inviteRouting = .unread
    }

    /// Read what the entered code is. Called whenever the code changes.
    public func decodeCode() {
        error = nil
        let code = codeAsEntered(text: codeInput)
        guard !code.isEmpty else {
            decoded = nil
            return
        }
        decoded = Result { try decoder(code) }
        // An invite goes on with a join under way for its library, else
        // asks the person's name.
        inviteRouting = .unread
        if case .join(_, let invite) = attempt, !isRunning {
            routeInvite(invite)
        }
    }

    /// Start what this screen opened to do at once: a join this device has
    /// under way, or finishing a restore from what core kept. The screen
    /// calls it as it appears.
    public func begin() {
        guard !isRunning else { return }
        switch entry {
        case .resume:
            start()
        case .finishRestore(let libraryId):
            launch { bootstrap in
                try await bootstrap.finishRestore(libraryId: libraryId)
            }
        case .resumeRestore(let restore):
            launch { bootstrap in
                try await bootstrap.resumeRestore(restore)
            }
        case .restoreSaved, .code, .restoreOver:
            break
        }
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
        launch { bootstrap in
            try await self.run(attempt, on: bootstrap, credentials: credentials)
        }
    }

    /// Run `work` on a new attempt, delivering what it produced or showing
    /// why it failed.
    private func launch(
        _ work:
            @escaping @MainActor (LibraryBootstrap) async throws ->
            BridgeBootstrapResult
    ) {
        let bootstrap = makeBootstrap()
        bootstrap.setWaitingScreenShown(waitingScreenShown)
        self.bootstrap = bootstrap
        let invite = attempt?.invite
        error = nil
        isRunning = true
        task = Task {
            defer {
                task = nil
                isRunning = false
            }
            do {
                let result = try await work(bootstrap)
                // A result that holds an open store is delivered even after a
                // cancel raced it, so the store is opened rather than leaked.
                deliver(result, invite: invite)
            }
            catch is CancellationError {
                logger.debug("bootstrap cancelled")
            }
            catch {
                logger.error("bootstrap failed: \(String(reflecting: error))")
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
            try await bootstrap.join(code: code, name: trimmedJoinName)
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
}

// MARK: - Joining

extension BootstrapCodeForm {
    /// The invite whose code read, when its person still has to give their
    /// name before joining: no join for its library is under way here.
    public var asksName: BridgeInviteInfo? {
        guard !isRunning, case .asksName = inviteRouting,
            case .join(_, let invite) = attempt
        else { return nil }
        return invite
    }

    /// The name as it is sent: without the spaces around it.
    private var trimmedJoinName: String {
        joinName.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Whether Join can run: a name is typed.
    public var canJoin: Bool {
        asksName != nil && !trimmedJoinName.isEmpty
    }

    /// Join as the person who typed `joinName`.
    public func join() {
        guard canJoin else { return }
        start()
    }

    /// Find where an invite whose code read goes: a join this device has
    /// under way for its library, which goes on at once, or else the name
    /// step.
    private func routeInvite(_ invite: BridgeInviteInfo) {
        inviteRouting = .checking
        routing?.cancel()
        let library = invite.metadata.libraryId
        let underWay = unfinishedJoins
        let libraryOf = joinLibrary
        routing = Task {
            do {
                let joins = try await underWay()
                guard !Task.isCancelled else { return }
                if let join = joins.first(where: { libraryOf($0) == library }) {
                    inviteRouting = .resumes(join)
                    if !isRunning { start() }
                }
                else {
                    inviteRouting = .asksName
                }
            }
            catch is CancellationError {}
            catch {
                logger.error(
                    "listing the joins under way failed: \(String(reflecting: error))"
                )
                inviteRouting = .unread
                self.error = DisplayError(error)
            }
        }
    }

    /// The person read that the join ended: the ended step goes, and the code
    /// screen is the first one again, with no code in it.
    public func acknowledgeEnd() {
        ended = nil
        codeInput = ""
        clearReading()
    }

    /// Whether the join's waiting screen shows: while it does, the join
    /// checks for approval every few seconds. Applied to the running join's
    /// control and to each one made after.
    public func setWaitingScreenShown(_ shown: Bool) {
        waitingScreenShown = shown
        bootstrap?.setWaitingScreenShown(shown)
    }
}

// MARK: - Results

extension BootstrapCodeForm {
    /// Continue the call this device has under way: a join after a cancel
    /// came as its request was sent, or the join or restore that kept this
    /// call from starting on the same library. A join goes first, as at
    /// launch.
    private func resumeUnfinished() {
        let (joins, restores) = (unfinishedJoins, unfinishedRestores)
        isRunning = true
        task = Task {
            defer {
                task = nil
                isRunning = false
            }
            do {
                let bootstrap = makeBootstrap()
                bootstrap.setWaitingScreenShown(waitingScreenShown)
                if let join = try await joins().first {
                    self.bootstrap = bootstrap
                    deliver(
                        try await bootstrap.resume(join),
                        invite: join.info()
                    )
                }
                else if let restore = try await restores().first {
                    self.bootstrap = bootstrap
                    deliver(
                        try await bootstrap.resumeRestore(restore),
                        invite: nil
                    )
                }
            }
            catch is CancellationError {
                logger.debug("resumed join cancelled")
            }
            catch {
                logger.error(
                    "resuming the join failed: \(String(reflecting: error))"
                )
                self.error = DisplayError(error)
            }
        }
    }

    /// Show what the attempt came to. `invite` is the join's, which ends
    /// without a library only a join does.
    private func deliver(
        _ result: BridgeBootstrapResult,
        invite: BridgeInviteInfo?
    ) {
        switch result {
        case .opened(let library):
            onLibraryReady(library, nil)
        case .published(let library, let warning):
            onLibraryReady(library, warning)
        case .declined:
            guard let invite else {
                preconditionFailure("only a join ends without letting it in")
            }
            ended = invite
        case .joinRequestSent, .callUnderWay:
            // After the running attempt's task has ended.
            Task { resumeUnfinished() }
        case .savedCodeRemoved:
            error = DisplayError(
                line: String(
                    localized:
                        "This restore code is no longer saved in iCloud Keychain."
                )
            )
        }
    }
}
