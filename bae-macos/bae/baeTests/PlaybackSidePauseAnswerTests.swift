import BaeKit
import Foundation
import Testing

/// Answering the side/disc pause prompt: the checkbox mirrors the
/// pause-between-sides setting and starts checked, so only an unchecked box
/// writes — it turns the setting off. Play resumes whatever the box says;
/// Close stops any countdown so the next side waits for Play.
@Suite("Playback.answerSidePausePrompt")
struct PlaybackSidePauseAnswerTests {
    private struct WriteFailed: Error {}

    /// What the answer sent, read at one moment.
    private struct SentSnapshot {
        let writes: [Bool]
        let resumes: Int
        let cancels: Int
    }

    /// Everything the answer sent: each pause-between-sides write, how many
    /// resumes, and how many countdown cancels.
    private final class Sent: @unchecked Sendable {
        private let lock = NSLock()
        private var writes: [Bool] = []
        private var resumes = 0
        private var cancels = 0

        func write(_ enabled: Bool) {
            lock.lock()
            writes.append(enabled)
            lock.unlock()
        }

        func resume() {
            lock.lock()
            resumes += 1
            lock.unlock()
        }

        func cancelCountdown() {
            lock.lock()
            cancels += 1
            lock.unlock()
        }

        var snapshot: SentSnapshot {
            lock.lock()
            defer { lock.unlock() }
            return SentSnapshot(
                writes: writes,
                resumes: resumes,
                cancels: cancels
            )
        }
    }

    private static func playback(_ sent: Sent, writeFails: Bool = false)
        -> Playback
    {
        Playback(
            resume: { sent.resume() },
            cancelSidePauseCountdown: { sent.cancelCountdown() },
            setPauseBetweenSides: { enabled in
                sent.write(enabled)
                if writeFails {
                    throw WriteFailed()
                }
            }
        )
    }

    @Test("Close with the box checked only stops the countdown")
    func closeCheckedOnlyStopsTheCountdown() throws {
        let sent = Sent()
        try Self.playback(sent)
            .answerSidePausePrompt(keepPausing: true, play: false)
        #expect(sent.snapshot.writes.isEmpty)
        #expect(sent.snapshot.resumes == 0)
        #expect(sent.snapshot.cancels == 1)
    }

    @Test("Play with the box checked only resumes")
    func playCheckedOnlyResumes() throws {
        let sent = Sent()
        try Self.playback(sent)
            .answerSidePausePrompt(keepPausing: true, play: true)
        #expect(sent.snapshot.writes.isEmpty)
        #expect(sent.snapshot.resumes == 1)
        #expect(sent.snapshot.cancels == 0)
    }

    @Test(
        "Close with the box unchecked turns the setting off and stops the countdown"
    )
    func closeUncheckedTurnsSettingOff() throws {
        let sent = Sent()
        try Self.playback(sent)
            .answerSidePausePrompt(keepPausing: false, play: false)
        #expect(sent.snapshot.writes == [false])
        #expect(sent.snapshot.resumes == 0)
        #expect(sent.snapshot.cancels == 1)
    }

    @Test("Play with the box unchecked turns the setting off and resumes")
    func playUncheckedTurnsSettingOffAndResumes() throws {
        let sent = Sent()
        try Self.playback(sent)
            .answerSidePausePrompt(keepPausing: false, play: true)
        #expect(sent.snapshot.writes == [false])
        #expect(sent.snapshot.resumes == 1)
        #expect(sent.snapshot.cancels == 0)
    }

    @Test("a failed write still resumes, then throws")
    func failedWriteStillResumes() {
        let sent = Sent()
        #expect(throws: WriteFailed.self) {
            try Self.playback(sent, writeFails: true)
                .answerSidePausePrompt(keepPausing: false, play: true)
        }
        #expect(sent.snapshot.resumes == 1)
    }
}
