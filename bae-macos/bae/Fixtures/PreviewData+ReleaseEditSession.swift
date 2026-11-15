import BaeKit

@testable import bae

extension PreviewData {
    /// An editor's session on `releaseId`, opened on a library holding it as
    /// `draft` (or on `library`).
    @MainActor
    static func releaseEditSession(
        releaseId: String = "release-preview",
        draft: BridgeReleaseEditDraft,
        library: PreviewReleaseLibrary? = nil
    ) -> ReleaseMetadataEditSession {
        let library = library ?? PreviewReleaseLibrary(draft: draft)
        return ReleaseMetadataEditSession(
            releaseId: releaseId,
            opened: library.opened(),
            commands: library.commands()
        )
    }
}
