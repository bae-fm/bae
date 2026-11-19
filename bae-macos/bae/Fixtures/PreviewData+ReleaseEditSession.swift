import BaeKit

@testable import bae

extension PreviewData {
    /// An editor's session on `releaseId`, opened on a library holding it as
    /// `release` (or on `library`).
    @MainActor
    static func releaseEditSession(
        releaseId: String = "release-preview",
        release: PreviewReleaseEdit,
        library: PreviewReleaseLibrary? = nil
    ) -> ReleaseMetadataEditSession {
        let library = library ?? PreviewReleaseLibrary(release: release)
        return ReleaseMetadataEditSession(
            releaseId: releaseId,
            opened: library.opened(),
            commands: library.commands()
        )
    }
}
