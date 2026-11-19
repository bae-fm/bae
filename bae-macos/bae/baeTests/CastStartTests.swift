import BaeKit
import Testing

@testable import bae

/// Starting a cast: one start for the app, which outlives the picker or page
/// that began it.
@MainActor
@Suite("Cast start")
struct CastStartTests {
    private struct Refused: Error {}

    private func receiver(_ id: String) -> BridgeCastReceiver {
        BridgeCastReceiver(
            id: id,
            name: id,
            kind: .cast,
            model: nil,
            address: "192.168.1.40",
            takesVolume: true
        )
    }

    @Test("a cast connects with no place open, and its failure is reported")
    func failureIsReported() async {
        let (answers, answer) = AsyncStream.makeStream(of: Bool.self)
        let cast = Cast(castTo: { _, _ in
            for await accepted in answers {
                guard accepted else { throw Refused() }
                return
            }
        })
        let (reports, reported) = AsyncStream.makeStream(of: (any Error).self)
        let start = CastStart(cast: cast, report: { reported.yield($0) })

        start.cast(to: receiver("kitchen"))
        #expect(start.connecting?.id == "kitchen")

        answer.yield(false)
        var errors = reports.makeAsyncIterator()
        let error = await errors.next()
        #expect(error is Refused)
        #expect(start.connecting == nil)
        #expect(start.started == nil)
    }

    @Test("a newer cast calls off the one still connecting")
    func newerCastCallsOffOlder() async throws {
        // Never answers: the kitchen's connect ends only when it is called off.
        let (silence, keepSilent) = AsyncStream.makeStream(of: Void.self)
        let (answers, answer) = AsyncStream.makeStream(of: Void.self)
        let (ends, ended) = AsyncStream.makeStream(of: String.self)
        let cast = Cast(castTo: { id, _ in
            if id == "kitchen" {
                for await _ in silence {}
                ended.yield(id)
                throw CancellationError()
            }
            for await _ in answers { return }
        })
        let start = CastStart(
            cast: cast,
            report: { Issue.record("a called-off cast reported \($0)") }
        )

        let kitchen = start.cast(to: receiver("kitchen"))
        let den = start.cast(to: receiver("den"))

        var endings = ends.makeAsyncIterator()
        #expect(await endings.next() == "kitchen")
        #expect(start.connecting?.id == "den")

        answer.yield(())
        try await Wait.until { start.connecting == nil }
        #expect(start.started == den)
        #expect(start.started != kitchen)
        keepSilent.finish()
    }

    @Test("a cast that connects closes only the place that asked for it")
    func onlyTheAskerSeesItStart() async throws {
        let (answers, answer) = AsyncStream.makeStream(of: Void.self)
        let cast = Cast(castTo: { _, _ in
            for await _ in answers { return }
        })
        let start = CastStart(cast: cast, report: { _ in })

        // A picker asks, is closed, and a second opens before the connect
        // finishes: the second asked nothing.
        let firstPickerAsked: CastStart.Attempt? = start.cast(
            to: receiver("kitchen")
        )
        let secondPickerAsked: CastStart.Attempt? = nil
        #expect(!start.didStart(firstPickerAsked))

        answer.yield(())
        try await Wait.until { start.connecting == nil }

        #expect(start.didStart(firstPickerAsked))
        #expect(!start.didStart(secondPickerAsked))
    }

    private func pinRequest(
        _ lastTry: BridgeAirPlayCodeRefused? = nil
    ) -> BridgeAirPlayCodeRequest {
        BridgeAirPlayCodeRequest(
            code: .pin(digits: 4),
            why: .firstPairing,
            lastTry: lastTry
        )
    }

    @Test("a receiver's ask opens the sheet, and Pair hands back the code")
    func askOpensTheSheetAndPairAnswers() async throws {
        let (codes, entered) = AsyncStream.makeStream(of: String.self)
        let (answers, answer) = AsyncStream.makeStream(of: Void.self)
        let request = pinRequest()
        let wrong = pinRequest(.wrongCode)
        let cast = Cast(castTo: { _, prompt in
            entered.yield(await prompt.code(request: request))
            entered.yield(await prompt.code(request: wrong))
            for await _ in answers { return }
        })
        let start = CastStart(
            cast: cast,
            report: { Issue.record("the cast reported \($0)") }
        )

        start.cast(to: receiver("kitchen"))
        try await Wait.until { start.codeSheet != nil }
        #expect(start.codeSheet?.request == request)
        #expect(start.codeSheet?.pairing == false)

        start.pair(code: "0482")
        #expect(start.codeSheet?.pairing == true)
        var typed = codes.makeAsyncIterator()
        #expect(await typed.next() == "0482")

        try await Wait.until { start.codeSheet?.request == wrong }
        #expect(start.codeSheet?.pairing == false)
        start.pair(code: "1234")
        #expect(await typed.next() == "1234")

        answer.yield(())
        try await Wait.until { start.connecting == nil }
        #expect(start.codeSheet == nil)
    }

    @Test("Cancel closes the sheet and calls the cast off")
    func cancelCallsTheCastOff() async throws {
        let (ends, ended) = AsyncStream.makeStream(of: Bool.self)
        let request = pinRequest()
        let cast = Cast(castTo: { _, prompt in
            _ = await prompt.code(request: request)
            ended.yield(Task.isCancelled)
            throw CancellationError()
        })
        let start = CastStart(
            cast: cast,
            report: { Issue.record("a cancelled cast reported \($0)") }
        )

        start.cast(to: receiver("kitchen"))
        try await Wait.until { start.codeSheet != nil }
        start.cancelCode()

        #expect(start.codeSheet == nil)
        #expect(start.connecting == nil)
        var endings = ends.makeAsyncIterator()
        #expect(await endings.next() == true)
        #expect(start.started == nil)
    }

    @Test("a receiver's refusal shows titled with the receiver, not reported")
    func refusalNamesTheReceiver() async throws {
        let cast = Cast(castTo: { _, _ in
            throw BridgeError.Diagnostic(
                category: .airPlayLockedOut(receiver: "Kitchen"),
                detail: "pair-setup M2: MaxTries"
            )
        })
        let start = CastStart(
            cast: cast,
            report: { Issue.record("a refusal was reported as \($0)") }
        )

        start.cast(to: receiver("kitchen"))
        try await Wait.until { start.connecting == nil }

        #expect(start.refusal?.receiver == "Kitchen")
        start.dismissRefusal()
        #expect(start.refusal == nil)
    }
}
