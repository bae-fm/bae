import BaeKit
import Foundation
import Testing

/// A host whose restore from a code reports the code it was given, whose
/// finishing of a replacement reports the library and call it was given, and
/// whose finishing of a restore onto this device reports the restore it was
/// given, each then ending as cancelled, or a restore from a code with
/// `restoreAnswer` when a test sets one; no other call is made by these
/// tests. Each test's is the one core host of the test process, which the
/// attempt's setup is made on: an attempt's reads end after the test does, as
/// a screen's end after it goes, so the host they end on lives for the
/// process, as the app's does.
private final class RestoreRecordingHost: BridgeHost, @unchecked Sendable {
    private static let core = TestHost.make()

    let restoredCodes: AsyncStream<String>
    private let record: AsyncStream<String>.Continuation
    let joinedCodes: AsyncStream<JoinCall>
    private let recordJoin: AsyncStream<JoinCall>.Continuation
    let resumedJoins: AsyncStream<ObjectIdentifier>
    private let recordResumedJoin: AsyncStream<ObjectIdentifier>.Continuation
    let finished: AsyncStream<FinishedReplacement>
    private let recordFinish: AsyncStream<FinishedReplacement>.Continuation
    let resumedRestores: AsyncStream<ObjectIdentifier>
    private let recordResume: AsyncStream<ObjectIdentifier>.Continuation
    /// What a restore from a code answers, in place of ending as cancelled.
    var restoreAnswer: BridgeBootstrapResult?
    /// What a join, new or resumed, answers, in place of ending as cancelled.
    var joinAnswer: BridgeBootstrapResult?

    init() {
        (restoredCodes, record) = AsyncStream.makeStream(of: String.self)
        (joinedCodes, recordJoin) = AsyncStream.makeStream(of: JoinCall.self)
        (resumedJoins, recordResumedJoin) = AsyncStream.makeStream(
            of: ObjectIdentifier.self
        )
        (finished, recordFinish) = AsyncStream.makeStream(
            of: FinishedReplacement.self
        )
        (resumedRestores, recordResume) = AsyncStream.makeStream(
            of: ObjectIdentifier.self
        )
        super.init(unsafeFromHandle: Self.core.uniffiCloneHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("a recording host is made by init()")
    }

    override func restoreFromCode(
        code: String,
        credentials _: BridgeS3Credentials?,
        deviceName _: String,
        setup _: BridgeLibrarySetup,
        cancellation _: BootstrapCancellation
    ) async throws -> BridgeBootstrapResult {
        record.yield(code)
        if let restoreAnswer { return restoreAnswer }
        throw CancellationError()
    }

    override func joinWithInvite(
        code: String,
        name: String,
        deviceName _: String,
        setup _: BridgeLibrarySetup,
        control _: BridgeJoinControl
    ) async throws -> BridgeBootstrapResult {
        recordJoin.yield(JoinCall(code: code, name: name))
        if let joinAnswer { return joinAnswer }
        throw CancellationError()
    }

    override func resumeJoin(
        join: BridgeUnfinishedJoin,
        setup _: BridgeLibrarySetup,
        control _: BridgeJoinControl
    ) async throws -> BridgeBootstrapResult {
        recordResumedJoin.yield(ObjectIdentifier(join))
        if let joinAnswer { return joinAnswer }
        throw CancellationError()
    }

    override func resumeReplacement(
        libraryId: String,
        by: BridgeReplacedBy,
        setup _: BridgeLibrarySetup,
        cancellation _: BootstrapCancellation
    ) async throws -> BridgeBootstrapResult {
        recordFinish.yield(FinishedReplacement(libraryId: libraryId, by: by))
        throw CancellationError()
    }

    override func resumeRestore(
        restore: BridgeUnfinishedRestore,
        setup _: BridgeLibrarySetup,
        cancellation _: BootstrapCancellation
    ) async throws -> BridgeBootstrapResult {
        recordResume.yield(ObjectIdentifier(restore))
        throw CancellationError()
    }
}

/// The code and name a join was given.
private struct JoinCall: Equatable, Sendable {
    let code: String
    let name: String
}

/// The library and call a finishing of a replacement was given.
private struct FinishedReplacement: Equatable, Sendable {
    let libraryId: String
    let by: BridgeReplacedBy
}

@MainActor
@Suite("BootstrapCodeForm")
struct BootstrapCodeFormTests {
    private let host = RestoreRecordingHost()

    private static let code = "CVR1-RESTORECODE"

    /// A restore code for an S3 library, holding the S3 key as the QR code
    /// the person's own device shows does, or not, as the code to type.
    private static func restoreCode(carriesS3Key: Bool) -> BridgeCodeInfo {
        .restore(metadata: metadata(carriesS3Key: carriesS3Key))
    }

    private static func metadata(carriesS3Key: Bool) -> BridgeCodeMetadata {
        BridgeCodeMetadata(
            libraryId: "library-1",
            memberId: "member-1",
            provider: .s3,
            location: .s3(
                bucket: "music-library",
                region: "us-east-1",
                endpoint: nil,
                profile: nil
            ),
            oauthAccess: nil,
            carriesS3Key: carriesS3Key
        )
    }

    /// An invite from Mara Quill to her library Low Tide Hours.
    private static let invite: BridgeCodeInfo = .invite(
        invite: BridgeInviteInfo(
            metadata: metadata(carriesS3Key: false),
            libraryName: "Low Tide Hours",
            inviter: "Mara Quill"
        )
    )

    private func form(
        reading info: BridgeCodeInfo,
        entry: BootstrapCodeForm.Entry = .code,
        unfinishedJoins: [BridgeUnfinishedJoin] = [],
        unfinishedRestores: [BridgeUnfinishedRestore] = []
    ) -> BootstrapCodeForm {
        let host = host
        return BootstrapCodeForm(
            entry: entry,
            decodeCode: { _ in info },
            makeBootstrap: {
                LibraryBootstrap(host: host, deviceName: "Test Mac")
            },
            unfinishedJoins: { unfinishedJoins },
            unfinishedRestores: { unfinishedRestores },
            // A join under way here is for the invite's library.
            joinLibrary: { _ in "library-1" },
            onLibraryReady: { _, _ in
                Issue.record("the recording host opens no library")
            }
        )
    }

    @Test("a scanned restore code adds this device at once")
    func scannedCodeStartsTheAdd() async {
        let form = form(reading: Self.restoreCode(carriesS3Key: true))

        form.acceptCode(Self.code)

        #expect(form.isRunning)
        var codes = host.restoredCodes.makeAsyncIterator()
        #expect(await codes.next() == Self.code)
    }

    @Test("a scanned S3 code without the key waits for the key")
    func scannedCodeWithoutKeyWaits() {
        let form = form(reading: Self.restoreCode(carriesS3Key: false))

        form.acceptCode(Self.code)

        #expect(!form.isRunning)
        #expect(form.attempt?.needsS3Key == true)
        #expect(!form.ready)
    }

    @Test("going back from the S3 key forgets the reading and keeps the code")
    func clearingTheReadingKeepsTheCode() {
        let form = form(reading: Self.restoreCode(carriesS3Key: false))
        form.acceptCode(Self.code)

        form.clearReading()

        #expect(form.attempt == nil)
        #expect(form.codeInput == codeInGroups(text: Self.code))
    }

    /// Wait for the form's look for a join under way for the invite's
    /// library to end in the name step.
    private func asksName(_ form: BootstrapCodeForm) async throws {
        try await Wait.until { form.asksName != nil }
    }

    @Test("an invite's code goes to the name step and starts nothing")
    func anInvitesCodeAsksTheName() async throws {
        let form = form(reading: Self.invite)

        form.edit(code: Self.code)
        form.decodeCode()
        try await asksName(form)

        guard case .join(let code, let invite) = form.attempt else {
            Issue.record("an invite decodes to a join")
            return
        }
        #expect(code == Self.code)
        #expect(invite.libraryName == "Low Tide Hours")
        #expect(form.asksName?.libraryName == "Low Tide Hours")
        #expect(!form.isRunning)
        #expect(!form.canJoin)
    }

    @Test("Join is off while the name is blank, and sends the trimmed name")
    func joinSendsTheTrimmedName() async throws {
        let form = form(reading: Self.invite)
        form.edit(code: Self.code)
        form.decodeCode()
        try await asksName(form)

        form.joinName = "   "
        #expect(!form.canJoin)
        form.join()
        #expect(!form.isRunning)

        form.joinName = "  Odile Fen \n"
        #expect(form.canJoin)
        form.join()

        var joined = host.joinedCodes.makeAsyncIterator()
        #expect(
            await joined.next() == JoinCall(code: Self.code, name: "Odile Fen")
        )
    }

    @Test("a join under way for the invite's library goes on without a name")
    func aJoinUnderWayAsksNoName() async throws {
        let underWay = BridgeUnfinishedJoin(
            noHandle: BridgeUnfinishedJoin.NoHandle()
        )
        let form = form(reading: Self.invite, unfinishedJoins: [underWay])

        form.edit(code: Self.code)
        form.decodeCode()

        // The join goes on at once: its resume is the event to wait for, and the
        // call ends as soon as the recording host answers, so the form is
        // not necessarily still running.
        var resumed = host.resumedJoins.makeAsyncIterator()
        #expect(await resumed.next() == ObjectIdentifier(underWay))
        #expect(form.asksName == nil)
    }

    @Test("a code typed in groups is passed on without the spaces")
    func groupedCodeIsPassedOnWithoutSpaces() async {
        let form = form(reading: Self.restoreCode(carriesS3Key: true))

        form.edit(code: "CVR1 REST ORE CODE")
        form.decodeCode()
        form.start()

        var codes = host.restoredCodes.makeAsyncIterator()
        #expect(await codes.next() == Self.code)
    }

    @Test("a code on the restore screen is read when the person continues")
    func restoreScreenReadsOnContinue() {
        let form = form(
            reading: Self.restoreCode(carriesS3Key: false),
            entry: .restoreOver(libraryId: "library-1")
        )

        form.edit(code: Self.code)

        #expect(form.attempt == nil)
        #expect(form.canContinue)

        form.submit()

        #expect(form.attempt?.needsS3Key == true)
        #expect(!form.canContinue)
    }

    @Test("continuing with a code that does not read says so")
    func continuingWithAnUnreadableCode() {
        let host = host
        let form = BootstrapCodeForm(
            entry: .restoreOver(libraryId: "library-1"),
            decodeCode: { _ in throw CocoaError(.fileReadCorruptFile) },
            makeBootstrap: {
                LibraryBootstrap(host: host, deviceName: "Test Mac")
            },
            unfinishedJoins: { [] },
            unfinishedRestores: { [] },
            onLibraryReady: { _, _ in }
        )

        form.edit(code: "half a code")

        #expect(form.decodeError == nil)

        form.submit()

        #expect(form.decodeError != nil)
    }

    @Test("an empty field cannot continue")
    func emptyFieldCannotContinue() {
        let form = form(reading: Self.restoreCode(carriesS3Key: true))

        #expect(!form.canContinue)
    }

    @Test("a typed code shows in groups of four")
    func typedCodeShowsInGroups() {
        let form = form(reading: Self.restoreCode(carriesS3Key: true))

        form.edit(code: Self.code)

        #expect(form.codeInput == "CVR1 REST OREC ODE")
    }

    @Test("a typed restore code waits for Add")
    func typedCodeWaitsForAdd() {
        let form = form(reading: Self.restoreCode(carriesS3Key: true))

        form.edit(code: Self.code)
        form.decodeCode()

        #expect(!form.isRunning)
        #expect(form.ready)
    }

    /// A restore of this device's copy that stopped partway finishes from
    /// what core kept as soon as its screen appears, with no code.
    @Test("finishing a restore begins at once without a code")
    func finishingARestoreBeginsAtOnce() async {
        let form = form(
            reading: Self.restoreCode(carriesS3Key: true),
            entry: .finishRestore(libraryId: "library-1")
        )

        form.begin()

        #expect(form.isRunning)
        var finished = host.finished.makeAsyncIterator()
        #expect(
            await finished.next()
                == FinishedReplacement(libraryId: "library-1", by: .restore)
        )
    }

    /// A restore onto this device that stopped after it recorded itself
    /// finishes from what core kept as soon as its screen appears, with no
    /// code.
    @Test("a stopped restore onto this device begins finishing at once")
    func resumingARestoreBeginsAtOnce() async {
        let restore = BridgeUnfinishedRestore(
            noHandle: BridgeUnfinishedRestore.NoHandle()
        )
        let form = form(
            reading: Self.restoreCode(carriesS3Key: true),
            entry: .resumeRestore(restore)
        )

        form.begin()

        #expect(form.isRunning)
        var resumed = host.resumedRestores.makeAsyncIterator()
        #expect(await resumed.next() == ObjectIdentifier(restore))
    }

    @Test(
        "a restore refused for another call under way goes on with that call"
    )
    func aRestoreRefusedForACallUnderWayGoesOnWithIt() async {
        let restore = BridgeUnfinishedRestore(
            noHandle: BridgeUnfinishedRestore.NoHandle()
        )
        host.restoreAnswer = .callUnderWay
        let form = form(
            reading: Self.restoreCode(carriesS3Key: true),
            unfinishedRestores: [restore]
        )

        form.acceptCode(Self.code)

        var resumed = host.resumedRestores.makeAsyncIterator()
        #expect(await resumed.next() == ObjectIdentifier(restore))
    }
}

extension BootstrapCodeFormTests {
    @Test("a join that ends without letting the device in shows the ended step")
    func aDeclinedJoinEnds() async throws {
        host.joinAnswer = .declined
        let form = form(reading: Self.invite)
        form.edit(code: Self.code)
        form.decodeCode()
        try await asksName(form)
        form.joinName = "Odile Fen"

        form.join()
        try await Wait.until { form.ended != nil }

        #expect(form.ended?.inviter == "Mara Quill")
        #expect(form.error == nil)
        #expect(!form.isRunning)

        form.acknowledgeEnd()

        #expect(form.ended == nil)
        #expect(form.codeInput.isEmpty)
        #expect(form.attempt == nil)
    }

    @Test("a resumed join that ends shows the ended step with its inviter")
    func aDeclinedResumedJoinEnds() async throws {
        host.joinAnswer = .declined
        let underWay = BridgeUnfinishedJoin(
            noHandle: BridgeUnfinishedJoin.NoHandle()
        )
        let form = form(reading: Self.invite, unfinishedJoins: [underWay])

        form.edit(code: Self.code)
        form.decodeCode()
        try await Wait.until { form.ended != nil }

        #expect(form.ended?.libraryName == "Low Tide Hours")
    }
}
