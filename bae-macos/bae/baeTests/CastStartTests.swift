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
        BridgeCastReceiver(id: id, name: id, kind: .cast)
    }

    @Test("a cast connects with no place open, and its failure is reported")
    func failureIsReported() async {
        let (answers, answer) = AsyncStream.makeStream(of: Bool.self)
        let cast = Cast(castTo: { _ in
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
    func newerCastCallsOffOlder() async {
        // Never answers: the kitchen's connect ends only when it is called off.
        let (silence, keepSilent) = AsyncStream.makeStream(of: Void.self)
        let (answers, answer) = AsyncStream.makeStream(of: Void.self)
        let (ends, ended) = AsyncStream.makeStream(of: String.self)
        let cast = Cast(castTo: { id in
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
        while start.connecting != nil { await Task.yield() }
        #expect(start.started == den)
        #expect(start.started != kitchen)
        keepSilent.finish()
    }

    @Test("a cast that connects closes only the place that asked for it")
    func onlyTheAskerSeesItStart() async {
        let (answers, answer) = AsyncStream.makeStream(of: Void.self)
        let cast = Cast(castTo: { _ in
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
        while start.connecting != nil { await Task.yield() }

        #expect(start.didStart(firstPickerAsked))
        #expect(!start.didStart(secondPickerAsked))
    }
}
