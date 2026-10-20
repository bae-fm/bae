#if DEBUG
    /// A saved restore code made from a fixture, for previews and tests: it
    /// answers `info()` itself instead of asking core. It has no core object
    /// behind it, so passing it to a bridge call crashes.
    public final class PreviewSavedRestoreCode: BridgeSavedRestoreCode,
        @unchecked Sendable
    {
        private let fixture: BridgeSavedRestoreCodeInfo

        public init(_ fixture: BridgeSavedRestoreCodeInfo) {
            self.fixture = fixture
            super.init(noHandle: NoHandle())
        }

        public required init(unsafeFromHandle handle: UInt64) {
            preconditionFailure(
                "a preview saved restore code is made from a fixture"
            )
        }

        public override func info() -> BridgeSavedRestoreCodeInfo {
            fixture
        }

        /// Saved codes the previews on every platform list: two libraries
        /// that share a name in different storage, and one in iCloud.
        public static let samples: [BridgeSavedRestoreCodeInfo] = [
            BridgeSavedRestoreCodeInfo(
                name: "Home Library",
                libraryId: "saved-01",
                location: .s3(
                    bucket: "music-library",
                    region: "us-east-1",
                    endpoint: "storage.example.com",
                    profile: nil,
                    prefix: ""
                ),
                hasS3Credentials: true
            ),
            BridgeSavedRestoreCodeInfo(
                name: "Home Library",
                libraryId: "saved-02",
                location: .dropbox(
                    location: .appFolder,
                    accountLabel: "ana@example.com"
                ),
                hasS3Credentials: false
            ),
            BridgeSavedRestoreCodeInfo(
                name: "Shared Library",
                libraryId: "saved-03",
                location: .cloudKit(
                    container: "iCloud.fm.bae.desktop",
                    owner: "owner",
                    zone: "zone"
                ),
                hasS3Credentials: false
            ),
        ]
    }
#endif
