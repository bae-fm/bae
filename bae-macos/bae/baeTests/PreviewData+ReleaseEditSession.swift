import BaeKit

@testable import bae

extension PreviewData {
    /// An editor's session on `releaseId`, opened on a library holding it as
    /// `seed` (or on `library`).
    @MainActor
    static func releaseEditSession(
        releaseId: String = "release-preview",
        seed: BridgeReleaseEditSeed,
        library: PreviewReleaseLibrary? = nil
    ) -> ReleaseMetadataEditSession {
        let library = library ?? PreviewReleaseLibrary(seed: seed)
        return ReleaseMetadataEditSession(
            releaseId: releaseId,
            opened: library.opened(),
            commands: library.commands()
        )
    }
}
