import BaeKit
import Testing

/// A host whose restore from a code reports the code it was given, then ends
/// as cancelled; no other call is made by these tests.
private final class RestoreRecordingHost: BridgeHost, @unchecked Sendable {
    let restoredCodes: AsyncStream<String>
    private let record: AsyncStream<String>.Continuation

    init() {
        (restoredCodes, record) = AsyncStream.makeStream(of: String.self)
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("a recording host has no core object")
    }

    override func restoreFromCode(
        code: String,
        credentials _: BridgeS3Credentials?,
        deviceName _: String,
        callback _: any BootstrapCallback,
        cancellation _: BootstrapCancellation
    ) async throws -> BridgeBootstrapResult {
        record.yield(code)
        throw CancellationError()
    }
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
                    profile: nil,
                    prefix: ""
                ),
                oauthAccess: nil,
                carriesS3Key: carriesS3Key
            )
        )
    }

    private func form(reading info: BridgeCodeInfo) -> BootstrapCodeForm {
        let host = host
        return BootstrapCodeForm(
            entry: .restore(nil),
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
}
