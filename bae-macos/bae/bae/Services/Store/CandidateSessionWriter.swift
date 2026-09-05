import BaeKit
import Foundation

/// Where a folder candidate's session writes go — which surface the pane
/// shows, the typed-search form, the banner's error — so core stores them
/// with the candidate and the next detail carries them back. The store holds
/// one of these; the app hands it the importer, and previews and tests run
/// with the inert one.
struct CandidateSessionWriter: Sendable {
    let setPresentation:
        @Sendable (String, BridgeMetadataPresentation) async throws -> Void
    let setSearchForm: @Sendable (String, BridgeSearchForm) async throws -> Void
    let setError: @Sendable (String, String?) async throws -> Void
    /// A write that failed is told to the person: the pane cannot show a state
    /// core never stored.
    let reportFailure: @MainActor @Sendable (Error) -> Void

    init(
        importer: Importer,
        reportFailure: @escaping @MainActor @Sendable (Error) -> Void
    ) {
        setPresentation = { key, presentation in
            try await importer.setCandidatePresentation(key, presentation)
        }
        setSearchForm = { key, form in
            try await importer.setCandidateSearchForm(key, form)
        }
        setError = { key, error in
            try await importer.setCandidatePaneError(key, error)
        }
        self.reportFailure = reportFailure
    }

    private init(
        setPresentation:
            @escaping @Sendable (String, BridgeMetadataPresentation)
            async throws -> Void,
        setSearchForm:
            @escaping @Sendable (String, BridgeSearchForm) async throws -> Void,
        setError: @escaping @Sendable (String, String?) async throws -> Void,
        reportFailure: @escaping @MainActor @Sendable (Error) -> Void
    ) {
        self.setPresentation = setPresentation
        self.setSearchForm = setSearchForm
        self.setError = setError
        self.reportFailure = reportFailure
    }

    /// Writes nothing and reports nothing: for a store no app is behind.
    static let inert = CandidateSessionWriter(
        setPresentation: { _, _ in },
        setSearchForm: { _, _ in },
        setError: { _, _ in },
        reportFailure: { _ in }
    )

    /// Records every write, for a test to read back.
    static func recording(
        _ record: @escaping @Sendable (CandidateSessionWrite) -> Void
    )
        -> CandidateSessionWriter
    {
        CandidateSessionWriter(
            setPresentation: { key, presentation in
                record(.presentation(key: key, presentation: presentation))
            },
            setSearchForm: { key, form in
                record(.searchForm(key: key, form: form))
            },
            setError: { key, error in record(.error(key: key, error: error)) },
            reportFailure: { _ in }
        )
    }
}

/// One session write, as a recording writer saw it.
enum CandidateSessionWrite: Equatable, Sendable {
    case presentation(key: String, presentation: BridgeMetadataPresentation)
    case searchForm(key: String, form: BridgeSearchForm)
    case error(key: String, error: String?)
}
