import BaeKit
import Foundation
import Testing

/// Answering the side/disc pause prompt: the checkbox mirrors the
/// pause-between-sides setting and starts checked, so only an unchecked box
/// writes — it turns the setting off — and Play resumes whatever the box says.
@Suite("Playback.answerSidePausePrompt")
struct PlaybackSidePauseAnswerTests {
    private struct WriteFailed: Error {}

    /// Everything the answer sent: each pause-between-sides write, and how
    /// many resumes.
    private final class Sent: @unchecked Sendable {
        private let lock = NSLock()
        private var writes: [Bool] = []
        private var resumes = 0

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

        var snapshot: (writes: [Bool], resumes: Int) {
            lock.lock()
            defer { lock.unlock() }
            return (writes, resumes)
        }
    }

    private static func playback(_ sent: Sent, writeFails: Bool = false)
        -> Playback
    {
        Playback(
            resume: { sent.resume() },
            setPauseBetweenSides: { enabled in
                sent.write(enabled)
                if writeFails {
                    throw WriteFailed()
                }
            }
        )
    }

    @Test("Close with the box checked changes nothing")
    func closeCheckedSendsNothing() throws {
        let sent = Sent()
        try Self.playback(sent)
            .answerSidePausePrompt(keepPausing: true, play: false)
        #expect(sent.snapshot.writes.isEmpty)
        #expect(sent.snapshot.resumes == 0)
    }

    @Test("Play with the box checked only resumes")
    func playCheckedOnlyResumes() throws {
        let sent = Sent()
        try Self.playback(sent)
            .answerSidePausePrompt(keepPausing: true, play: true)
        #expect(sent.snapshot.writes.isEmpty)
        #expect(sent.snapshot.resumes == 1)
    }

    @Test("Close with the box unchecked turns the setting off")
    func closeUncheckedTurnsSettingOff() throws {
        let sent = Sent()
        try Self.playback(sent)
            .answerSidePausePrompt(keepPausing: false, play: false)
        #expect(sent.snapshot.writes == [false])
        #expect(sent.snapshot.resumes == 0)
    }

    @Test("Play with the box unchecked turns the setting off and resumes")
    func playUncheckedTurnsSettingOffAndResumes() throws {
        let sent = Sent()
        try Self.playback(sent)
            .answerSidePausePrompt(keepPausing: false, play: true)
        #expect(sent.snapshot.writes == [false])
        #expect(sent.snapshot.resumes == 1)
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
