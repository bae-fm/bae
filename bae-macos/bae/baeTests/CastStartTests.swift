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
        BridgeCastReceiver(id: id, name: id, kind: .cast, casting: false)
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

        start.cast(
            to: receiver("kitchen"),
            onStarted: { Issue.record("a refused cast started") }
        )
        #expect(start.connecting?.id == "kitchen")

        answer.yield(false)
        var errors = reports.makeAsyncIterator()
        let error = await errors.next()
        #expect(error is Refused)
        #expect(start.connecting == nil)
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
        let (starts, startedOn) = AsyncStream.makeStream(of: String.self)
        let start = CastStart(
            cast: cast,
            report: { Issue.record("a called-off cast reported \($0)") }
        )

        start.cast(
            to: receiver("kitchen"),
            onStarted: { Issue.record("the called-off cast started") }
        )
        start.cast(to: receiver("den"), onStarted: { startedOn.yield("den") })

        var endings = ends.makeAsyncIterator()
        #expect(await endings.next() == "kitchen")
        #expect(start.connecting?.id == "den")

        answer.yield(())
        var started = starts.makeAsyncIterator()
        #expect(await started.next() == "den")
        #expect(start.connecting == nil)
        keepSilent.finish()
    }
}
