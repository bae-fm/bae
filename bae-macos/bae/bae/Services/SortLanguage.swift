import BaeKit
import Foundation

/// Tells core the language the app is shown in, so the library orders titles,
/// artists and composers as readers of that language expect: at launch, and
/// again whenever the system's language or region settings change. Core
/// writes the new order whole or not at all and a language it already has
/// costs nothing, so each notification is simply passed on.
@MainActor
final class SortLanguage {
    private let set: @Sendable (_ tag: String) async throws -> Void
    private let languages: () -> [String]
    private let showError: @MainActor (Error) -> Void

    init(
        set: @escaping @Sendable (String) async throws -> Void,
        languages: @escaping () -> [String] = { Locale.preferredLanguages },
        showError: @escaping @MainActor (Error) -> Void
    ) {
        self.set = set
        self.languages = languages
        self.showError = showError
    }

    convenience init(
        handle: any AppHandleProtocol,
        showError: @escaping @MainActor (Error) -> Void
    ) {
        self.init(
            set: { try await handle.setSortLanguage(tag: $0) },
            showError: showError
        )
    }

    /// Apply the language now and after each change of the system's.
    func start() {
        apply()
        // The center keeps the observer for the app's life, as this object is.
        NotificationCenter.default.addObserver(
            forName: NSLocale.currentLocaleDidChangeNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.apply() }
        }
    }

    /// The first language the person prefers; `und`, the root order, when the
    /// system lists none.
    var tag: String {
        languages().first ?? "und"
    }

    private func apply() {
        let tag = tag
        let set = set
        let showError = showError
        Task { @MainActor in
            do { try await set(tag) }
            catch { showError(error) }
        }
    }
}
