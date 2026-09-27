import BaeKit
import Foundation

/// Where a folder candidate's session writes go — the pane's moves and the
/// typed-search form — so core stores them with the candidate and the next
/// detail carries them back. The store holds
/// one of these; the app hands it the importer, and previews and tests run
/// with the inert one.
struct CandidateSessionWriter: Sendable {
    let movePane: @Sendable (String, BridgePaneMove) async throws -> Void
    let setSearchForm: @Sendable (String, BridgeSearchForm) async throws -> Void
    /// A failure no pane states is told to the person: a session write that
    /// failed, since the pane cannot show a state core never stored, or a pane
    /// command whose failure core could not store.
    let reportFailure: @MainActor @Sendable (Error) -> Void

    init(
        importer: Importer,
        reportFailure: @escaping @MainActor @Sendable (Error) -> Void
    ) {
        movePane = { key, paneMove in
            try await importer.moveCandidatePane(key, paneMove)
        }
        setSearchForm = { key, form in
            try await importer.setCandidateSearchForm(key, form)
        }
        self.reportFailure = reportFailure
    }

    init(
        movePane:
            @escaping @Sendable (String, BridgePaneMove) async throws ->
            Void,
        setSearchForm:
            @escaping @Sendable (String, BridgeSearchForm) async throws -> Void,
        reportFailure: @escaping @MainActor @Sendable (Error) -> Void
    ) {
        self.movePane = movePane
        self.setSearchForm = setSearchForm
        self.reportFailure = reportFailure
    }

    /// Writes nothing and reports nothing: for a store no app is behind.
    static let inert = CandidateSessionWriter(
        movePane: { _, _ in },
        setSearchForm: { _, _ in },
        reportFailure: { _ in }
    )
}
