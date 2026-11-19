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
    /// Why the receiver of the last start refused it, until the person
    /// dismisses it or casts again.
    public private(set) var refusal: Refusal?
    private var attempts = 0
    private let cast: Cast
    private let report: @MainActor (any Error) -> Void
    private var task: Task<Void, Never>?
    /// The open sheet, while one is.
    private var code: CodeState?

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
        fileprivate let asked: Asked
        public let pairing: Bool

        public var receiver: BridgeCastReceiver { asked.receiver }
        public var request: BridgeAirPlayCodeRequest { asked.request }
        public var id: Int { asked.attempt.number }
    }

    /// The sheet that asks for the code the receiver of the cast connecting
    /// wants, while it is open.
    public var codeSheet: CodeSheet? {
        switch code {
        case .asking(let asked, _): CodeSheet(asked: asked, pairing: false)
        case .pairing(let asked): CodeSheet(asked: asked, pairing: true)
        case nil: nil
        }
    }

    /// What a receiver of the cast `attempt` asked for.
    fileprivate struct Asked: Equatable {
        let receiver: BridgeCastReceiver
        let request: BridgeAirPlayCodeRequest
        let attempt: Attempt
    }

    /// Where the open sheet is.
    private enum CodeState {
        /// The sheet waits for the person's code, which `answer` takes to
        /// core.
        case asking(Asked, answer: CheckedContinuation<String, Never>)
        /// The person's code is out, and the sheet shows Pairing… until core
        /// asks again or the cast ends.
        case pairing(Asked)

        var asked: Asked {
            switch self {
            case .asking(let asked, _), .pairing(let asked): asked
            }
        }
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
        // Core holds the prompt for the whole cast, so the prompt holds only
        // the end of the channel this start reads its requests from.
        let (asks, asker) = AsyncStream.makeStream(of: CodeAsk.self)
        let prompt = CastCodePrompt(asks: asker)
        let showing = Task {
            for await ask in asks {
                take(ask, receiver: receiver, attempt: attempt)
            }
        }
        task = Task {
            let outcome: Result<Void, any Error>
            do {
                try await cast.castTo(receiver.id, prompt)
                outcome = .success(())
            }
            catch {
                outcome = .failure(error)
            }
            // Every request the cast sent is shown before its sheet closes.
            asker.finish()
            await showing.value
            closeCodeSheet(of: attempt)
            end(attempt, outcome)
        }
        return attempt
    }

    /// Hand core the code the person entered in the open sheet. The sheet
    /// shows Pairing… until core asks again or the cast ends.
    public func pair(code entered: String) {
        guard case .asking(let asked, let answer) = code else { return }
        code = .pairing(asked)
        answer.resume(returning: entered)
    }

    /// The person cancelled the open sheet: close it and call off the cast
    /// it asks for, which casts nothing.
    public func cancelCode() {
        guard code != nil else { return }
        task?.cancel()
        task = nil
        connecting = nil
        closeCodeSheet()
    }

    /// The person dismissed the refusal.
    public func dismissRefusal() {
        refusal = nil
    }

    /// Show what the prompt of the cast `attempt` sent.
    private func take(
        _ ask: CodeAsk,
        receiver: BridgeCastReceiver,
        attempt: Attempt
    ) {
        switch ask {
        case .ask(let request, let answer):
            // A cast another start replaced asks nothing more.
            guard attempt.number == attempts else {
                answer.resume(returning: "")
                return
            }
            if case .asking(_, let unanswered) = code {
                unanswered.resume(returning: "")
            }
            code = .asking(
                Asked(receiver: receiver, request: request, attempt: attempt),
                answer: answer
            )
        case .withdrawn:
            closeCodeSheet(of: attempt)
        }
    }

    /// Close the sheet, or only the sheet of `attempt`, answering a request
    /// still open with nothing: whatever asked has stopped waiting.
    private func closeCodeSheet(of attempt: Attempt? = nil) {
        guard let open = code else { return }
        if let attempt, open.asked.attempt != attempt { return }
        code = nil
        if case .asking(_, let answer) = open {
            answer.resume(returning: "")
        }
    }

    /// Show how the cast `attempt` ended, unless a newer start or Cancel
    /// called it off.
    private func end(_ attempt: Attempt, _ outcome: Result<Void, any Error>) {
        switch outcome {
        case .success:
            guard !Task.isCancelled else { return }
            connecting = nil
            started = attempt
        case .failure(is CancellationError):
            return
        case .failure(let error):
            // A newer start owns `connecting` now.
            guard !Task.isCancelled else { return }
            connecting = nil
            if let refused = receiverRefusal(error, attempt: attempt) {
                refusal = refused
            }
            else {
                report(error)
            }
        }
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
    #endif
}

#if DEBUG
    extension CastStart.CodeSheet {
        /// The sheet asking `request` of `receiver`, with Pairing… once
        /// `pairing`, as the sheet's pictures draw it.
        public static func drawn(
            _ request: BridgeAirPlayCodeRequest,
            of receiver: BridgeCastReceiver,
            pairing: Bool = false
        ) -> CastStart.CodeSheet {
            CastStart.CodeSheet(
                asked: CastStart.Asked(
                    receiver: receiver,
                    request: request,
                    attempt: CastStart.Attempt(number: 0)
                ),
                pairing: pairing
            )
        }
    }
#endif

/// What a cast's prompt sends its start: a request to show, with what takes
/// the person's code back to core, or word that core stopped waiting.
private enum CodeAsk: Sendable {
    case ask(
        BridgeAirPlayCodeRequest,
        answer: CheckedContinuation<String, Never>
    )
    case withdrawn
}

/// The prompt one cast asks for its receiver's code through: the sending end
/// of the channel its start shows each request from.
private final class CastCodePrompt: AirPlayCodePromptCallback {
    private let asks: AsyncStream<CodeAsk>.Continuation

    init(asks: AsyncStream<CodeAsk>.Continuation) {
        self.asks = asks
    }

    func code(request: BridgeAirPlayCodeRequest) async -> String {
        await withTaskCancellationHandler {
            await withCheckedContinuation { answer in
                // A cast already called off, or one whose start stopped
                // reading, asks nothing more.
                guard !Task.isCancelled,
                    case .enqueued = asks.yield(.ask(request, answer: answer))
                else {
                    answer.resume(returning: "")
                    return
                }
            }
        } onCancel: {
            asks.yield(.withdrawn)
        }
    }
}
