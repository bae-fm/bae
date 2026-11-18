import Observation

/// The cast the person started last, while it connects. There is one for the
/// app, not one per place that lists receivers: the picker and the Casting
/// settings page show the same start, and closing the one that began it leaves
/// it connecting. A newer start calls off the one before it. A start that
/// connects shows as `started`; a place that began one closes itself when
/// `started` is the `Attempt` it was given, so a place opened later never
/// closes for a start it did not ask for.
///
/// An AirPlay receiver that wants a PIN or a password asks the person through
/// `codeSheet`, which the app shows over its main window while it is set: the
/// person's code goes back with `pair(code:)`, and `cancelCode()` calls the
/// cast off. A receiver that refuses the cast says why as `refusal`; any other
/// failed start says why through `report`, wherever the person is by then.
@MainActor
@Observable
public final class CastStart {
    /// The receiver a cast is connecting to, while it does.
    public private(set) var connecting: BridgeCastReceiver?
    /// The latest start that reached playback on its receiver.
    public private(set) var started: Attempt?
    /// The sheet that asks for the code the receiver of the cast connecting
    /// wants, while it is open.
    public private(set) var codeSheet: CodeSheet?
    /// Why the receiver of the last start refused it, until the person
    /// dismisses it or casts again.
    public private(set) var refusal: Refusal?
    private var attempts = 0
    private let cast: Cast
    private let report: @MainActor (any Error) -> Void
    private var task: Task<Void, Never>?
    /// What answers the open sheet's request, once the person presses Pair.
    private var codeAnswer: CheckedContinuation<String, Never>?

    /// Starts go through `cast`. Why one failed goes to `report`, the app's
    /// place for errors, which shows nothing for an error core gives no line:
    /// a cast called off by stopping casting, turning it off, or another cast.
    public init(cast: Cast, report: @escaping @MainActor (any Error) -> Void) {
        self.cast = cast
        self.report = report
    }

    /// One call to `cast(to:)`. `started` equals it once that start connects.
    public struct Attempt: Equatable, Sendable {
        fileprivate let number: Int
    }

    /// What the open sheet asks: the receiver, core's request, and whether
    /// the person's code is out, so the sheet shows Pairing… until core asks
    /// again or the cast ends. One cast's sheet keeps its identity across its
    /// requests, so it stays open between a wrong code and the next.
    public struct CodeSheet: Equatable, Identifiable {
        public let receiver: BridgeCastReceiver
        public let request: BridgeAirPlayCodeRequest
        public let pairing: Bool
        fileprivate let attempt: Attempt

        public var id: Int { attempt.number }
    }

    /// A receiver's refusal of a cast: the receiver's name, which the alert's
    /// title names, and core's line.
    public struct Refusal: Equatable, Identifiable {
        public let receiver: String
        public let error: DisplayError
        fileprivate let attempt: Attempt

        public var id: Int { attempt.number }
    }

    /// Whether `attempt`, what a place was given when it asked, is the start
    /// that connected. A place that asked nothing, or asked an earlier start
    /// that a later one replaced, gets false.
    public func didStart(_ attempt: Attempt?) -> Bool {
        attempt != nil && attempt == started
    }

    /// Switch playback to `receiver`, calling off the start an earlier call
    /// began. Returns the attempt `started` becomes once playback is on the
    /// receiver.
    @discardableResult
    public func cast(to receiver: BridgeCastReceiver) -> Attempt {
        task?.cancel()
        closeCodeSheet()
        refusal = nil
        connecting = receiver
        attempts += 1
        let attempt = Attempt(number: attempts)
        let cast = cast
        let prompt = CastCodePrompt(
            start: self,
            receiver: receiver,
            attempt: attempt
        )
        task = Task {
            do {
                try await cast.castTo(receiver.id, prompt)
                guard !Task.isCancelled else { return }
                closeCodeSheet(of: attempt)
                connecting = nil
                started = attempt
            }
            catch is CancellationError {
                return
            }
            catch {
                // A newer start owns `connecting` now.
                guard !Task.isCancelled else { return }
                closeCodeSheet(of: attempt)
                connecting = nil
                if let refused = receiverRefusal(error, attempt: attempt) {
                    refusal = refused
                }
                else {
                    report(error)
                }
            }
        }
        return attempt
    }

    /// Hand core the code the person entered in the open sheet. The sheet
    /// shows Pairing… until core asks again or the cast ends.
    public func pair(code: String) {
        guard let sheet = codeSheet, let answer = codeAnswer else { return }
        codeAnswer = nil
        codeSheet = CodeSheet(
            receiver: sheet.receiver,
            request: sheet.request,
            pairing: true,
            attempt: sheet.attempt
        )
        answer.resume(returning: code)
    }

    /// The person cancelled the open sheet: close it and call off the cast
    /// it asks for, which casts nothing.
    public func cancelCode() {
        guard codeSheet != nil else { return }
        task?.cancel()
        task = nil
        connecting = nil
        closeCodeSheet()
    }

    /// The person dismissed the refusal.
    public func dismissRefusal() {
        refusal = nil
    }

    /// What the prompt of the cast `attempt` asks: show `request` in the
    /// sheet and wait for Pair. A cast that ends while this waits cancels
    /// this call, which closes the sheet.
    fileprivate func askCode(
        _ request: BridgeAirPlayCodeRequest,
        receiver: BridgeCastReceiver,
        attempt: Attempt
    ) async -> String {
        // A cast another start replaced asks nothing more; core has dropped
        // what waits for this answer.
        guard attempt.number == attempts, !Task.isCancelled else { return "" }
        codeAnswer?.resume(returning: "")
        codeAnswer = nil
        codeSheet = CodeSheet(
            receiver: receiver,
            request: request,
            pairing: false,
            attempt: attempt
        )
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                guard !Task.isCancelled else {
                    continuation.resume(returning: "")
                    return
                }
                codeAnswer = continuation
            }
        } onCancel: {
            Task { @MainActor in self.closeCodeSheet(of: attempt) }
        }
    }

    /// Close the sheet, or only the sheet of `attempt`, answering a request
    /// still open with nothing: whatever asked has stopped waiting.
    private func closeCodeSheet(of attempt: Attempt? = nil) {
        if let attempt, codeSheet?.attempt != attempt { return }
        codeSheet = nil
        codeAnswer?.resume(returning: "")
        codeAnswer = nil
    }

    /// `error` as a receiver's refusal of `attempt`, when core says it is
    /// one.
    private func receiverRefusal(
        _ error: any Error,
        attempt: Attempt
    ) -> Refusal? {
        guard let bridgeError = error as? BridgeError,
            let receiver = bridgeErrorRefusingReceiver(error: bridgeError),
            let shown = DisplayError(bridgeError)
        else { return nil }
        return Refusal(receiver: receiver, error: shown, attempt: attempt)
    }

    #if DEBUG
        /// A start over `Cast.stub()`, whose casts start at once.
        public static func stub() -> CastStart {
            CastStart(cast: .stub(), report: { _ in })
        }

        /// A start whose sheet shows `request` for `receiver`, as the
        /// sheet's pictures draw it.
        public static func asking(
            _ request: BridgeAirPlayCodeRequest,
            of receiver: BridgeCastReceiver,
            pairing: Bool = false
        ) -> CastStart {
            let start = stub()
            start.codeSheet = CodeSheet(
                receiver: receiver,
                request: request,
                pairing: pairing,
                attempt: Attempt(number: 0)
            )
            return start
        }

        /// A start whose last cast `receiver` refused with `error`, as the
        /// refusal's pictures draw it.
        public static func refused(
            by receiver: String,
            with error: DisplayError
        ) -> CastStart {
            let start = stub()
            start.refusal = Refusal(
                receiver: receiver,
                error: error,
                attempt: Attempt(number: 0)
            )
            return start
        }
    #endif
}

/// The prompt one cast asks for its receiver's code through: the start's
/// sheet.
private final class CastCodePrompt: AirPlayCodePromptCallback {
    let start: CastStart
    let receiver: BridgeCastReceiver
    let attempt: CastStart.Attempt

    init(
        start: CastStart,
        receiver: BridgeCastReceiver,
        attempt: CastStart.Attempt
    ) {
        self.start = start
        self.receiver = receiver
        self.attempt = attempt
    }

    func code(request: BridgeAirPlayCodeRequest) async -> String {
        await start.askCode(request, receiver: receiver, attempt: attempt)
    }
}
