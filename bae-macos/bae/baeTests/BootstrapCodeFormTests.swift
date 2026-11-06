import BaeKit
import Testing

/// A host whose restore from a code reports the code it was given, and whose
/// finishing of a replacement reports the library and call it was given,
/// each then ending as cancelled; no other call is made by these tests. Each
/// test's is the one core host of the test process, which the attempt's setup
/// is made on: an attempt's reads end after the test does, as a screen's end
/// after it goes, so the host they end on lives for the process, as the app's
/// does.
private final class RestoreRecordingHost: BridgeHost, @unchecked Sendable {
    private static let core = TestHost.make()

    let restoredCodes: AsyncStream<String>
    private let record: AsyncStream<String>.Continuation
    let finished: AsyncStream<FinishedReplacement>
    private let recordFinish: AsyncStream<FinishedReplacement>.Continuation

    init() {
        (restoredCodes, record) = AsyncStream.makeStream(of: String.self)
        (finished, recordFinish) = AsyncStream.makeStream(
            of: FinishedReplacement.self
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

    private static let code = "CVR1-RESTORE-CODE"

    /// A restore code for an S3 library, holding the S3 key as the QR code
    /// the person's own device shows does, or not, as the code to type.
    private static func restoreCode(carriesS3Key: Bool) -> BridgeCodeInfo {
        .restore(
            metadata: BridgeCodeMetadata(
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
        )
    }

    private func form(
        reading info: BridgeCodeInfo,
        entry: BootstrapCodeForm.Entry = .restore(nil)
    ) -> BootstrapCodeForm {
        let host = host
        return BootstrapCodeForm(
            entry: entry,
            decodeCode: { _ in info },
            makeBootstrap: {
                LibraryBootstrap(host: host, deviceName: "Test Mac")
            },
            unfinishedJoins: { [] },
            onLibraryReady: { _, _ in
                Issue.record("the recording host opens no library")
            }
        )
    }

    @Test("a scanned restore code adds this device at once")
    func scannedCodeStartsTheAdd() async {
        let form = form(reading: Self.restoreCode(carriesS3Key: true))

        form.acceptScan(Self.code)

        #expect(form.isRunning)
        var codes = host.restoredCodes.makeAsyncIterator()
        #expect(await codes.next() == Self.code)
    }

    @Test("a scanned S3 code without the key waits for the key")
    func scannedCodeWithoutKeyWaits() {
        let form = form(reading: Self.restoreCode(carriesS3Key: false))

        form.acceptScan(Self.code)

        #expect(!form.isRunning)
        #expect(form.attempt?.needsS3Key == true)
        #expect(!form.ready)
    }

    @Test("a typed restore code waits for Add")
    func typedCodeWaitsForAdd() {
        let form = form(reading: Self.restoreCode(carriesS3Key: true))

        form.codeInput = Self.code
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
}
