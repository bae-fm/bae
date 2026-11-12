import Foundation
import Observation
import os.log

private let logger = Logger.bae("StorageFolderBrowser")

/// Dropbox or OneDrive setup's state: sign in, then choose the empty folder
/// the library lives in, or make one. The two providers share it: a provider
/// is its facts (`Provider`) and its calls (`Operations`), and the view draws
/// both the same. The provider calls are the app's, since only an edition with
/// OAuth providers has them.
@MainActor
@Observable
public final class StorageFolderBrowser {
    public enum Provider {
        case dropbox
        case oneDrive

        public var cloudProvider: BridgeCloudProvider {
            switch self {
            case .dropbox: .dropbox
            case .oneDrive: .oneDrive
            }
        }
    }

    /// The signed-in provider calls folder choice runs on.
    public struct Operations: Sendable {
        let signIn: @Sendable (BridgeCloudProvider) async throws -> Void
        /// The first folders to choose from, with the path to them.
        let top: @Sendable () async throws -> Listing
        /// The folders inside one; nil for a provider whose folders are one
        /// flat list.
        let inside:
            (
                @Sendable (BridgeStorageFolder) async throws ->
                    [BridgeStorageFolder]
            )?
        /// Make a folder named so in the folder shown, or at the top when
        /// the provider's folders are flat, and return it.
        let make:
            @Sendable (BridgeStorageFolder?, String) async throws ->
                BridgeStorageFolder

        public init(
            signIn:
                @escaping @Sendable (BridgeCloudProvider) async throws -> Void,
            top: @escaping @Sendable () async throws -> Listing,
            inside:
                (
                    @Sendable (BridgeStorageFolder) async throws ->
                        [BridgeStorageFolder]
                )?,
            make:
                @escaping @Sendable (BridgeStorageFolder?, String) async throws
                -> BridgeStorageFolder
        ) {
            self.signIn = signIn
            self.top = top
            self.inside = inside
            self.make = make
        }
    }

    /// The folders signed in so far, and where they are.
    public struct Listing: Sendable {
        /// The path from the root to the folder shown, for a provider whose
        /// folders nest; empty for a flat list.
        public let path: [BridgeStorageFolder]
        public let folders: [BridgeStorageFolder]

        public init(path: [BridgeStorageFolder], folders: [BridgeStorageFolder])
        {
            self.path = path
            self.folders = folders
        }
    }

    /// The provider call running, which the step names while it runs.
    public enum Work: Equatable {
        /// The person signs in through their browser.
        case signingIn
        /// The provider lists the folders to choose from.
        case listing
        /// The provider makes the folder the person named, shown by its name.
        case makingFolder(String)
    }

    public let provider: Provider
    /// Nil until sign-in lists the first folders.
    public private(set) var listing: Listing?
    /// The folder chosen from the list; Set up uses it.
    public private(set) var selected: BridgeStorageFolder?
    /// Whether the name field for a new folder shows.
    public private(set) var isNaming = false
    public var newFolderName = ""
    /// The name the provider refused because a folder of that name is already
    /// there; nil until it refuses one.
    public private(set) var takenName: String?
    /// The folder setup found holding other files, which the person
    /// replaces by choosing another or making one.
    public private(set) var notEmpty: BridgeStorageFolder?
    /// The call running; nil while none is.
    public private(set) var working: Work?
    public private(set) var error: DisplayError?

    @ObservationIgnored
    private let operations: Operations
    @ObservationIgnored
    private var task: Task<Void, Never>?

    public init(provider: Provider, operations: Operations) {
        self.provider = provider
        self.operations = operations
    }

    #if DEBUG
        /// A browser already signed in and showing `listing`, for a shot or
        /// a shot; its calls all fail.
        public init(
            staged provider: Provider,
            listing: Listing,
            nests: Bool,
            selected: BridgeStorageFolder? = nil,
            isNaming: Bool = false,
            newFolderName: String = "",
            notEmpty: BridgeStorageFolder? = nil
        ) {
            self.provider = provider
            let inside:
                (
                    @Sendable (BridgeStorageFolder) async throws ->
                        [BridgeStorageFolder]
                )?
            if nests {
                inside = { _ in throw CancellationError() }
            }
            else {
                inside = nil
            }
            self.operations = Operations(
                signIn: { _ in throw CancellationError() },
                top: { throw CancellationError() },
                inside: inside,
                make: { _, _ in throw CancellationError() }
            )
            self.listing = listing
            self.selected = selected
            self.isNaming = isNaming
            self.newFolderName = newFolderName
            self.notEmpty = notEmpty
        }
    #endif

    /// The folder Set up uses: the one chosen, unless setup just found it
    /// holding other files.
    public var target: BridgeStorageFolder? {
        guard let selected, selected != notEmpty else { return nil }
        return selected
    }

    /// Whether a folder opens into the folders inside it, so the list shows
    /// where the person is.
    public var nests: Bool { operations.inside != nil }

    public var isWorking: Bool { working != nil }

    public var canOpenParent: Bool { (listing?.path.count ?? 0) > 1 }

    public var canCreateFolder: Bool {
        !isWorking
            && !newFolderName.trimmingCharacters(in: .whitespaces).isEmpty
    }

    /// The path to the folder shown, by the names the provider gives.
    public var pathNames: [String] { listing?.path.map(\.name) ?? [] }

    public func select(_ folder: BridgeStorageFolder) {
        selected = folder
    }

    /// Sign in through the bridge's presenter, then list the first folders.
    public func signIn() {
        let operations = operations
        let provider = provider
        run(.signingIn) {
            try await operations.signIn(provider.cloudProvider)
            self.working = .listing
            return try await operations.top()
        }
    }

    /// List the folders at the top again, after the person made or shared
    /// one elsewhere.
    public func lookAgain() {
        let operations = operations
        selected = nil
        run(.listing) { try await operations.top() }
    }

    public func open(_ folder: BridgeStorageFolder) {
        guard let path = listing?.path else {
            preconditionFailure("folders show only after sign-in")
        }
        guard let inside = operations.inside else {
            preconditionFailure("only a nesting provider opens a folder")
        }
        selected = nil
        run(.listing) {
            Listing(path: path + [folder], folders: try await inside(folder))
        }
    }

    public func openParent() {
        guard let path = listing?.path, path.count > 1 else {
            preconditionFailure("Up shows only below the root")
        }
        let parentPath = Array(path.dropLast())
        guard let parent = parentPath.last else {
            preconditionFailure("a path below the root has a parent")
        }
        guard let inside = operations.inside else {
            preconditionFailure("only a nesting provider has a parent")
        }
        selected = nil
        run(.listing) {
            Listing(path: parentPath, folders: try await inside(parent))
        }
    }

    /// Show the name field for a new folder.
    public func startNaming() {
        isNaming = true
    }

    public func stopNaming() {
        isNaming = false
        newFolderName = ""
    }

    /// Make a folder named `newFolderName` in the folder shown, add it to the
    /// list and choose it. A name already taken is not a failure of the call:
    /// the list stays as it was and `takenName` names it.
    public func createFolder() {
        guard let listing else {
            preconditionFailure("folders are made only after sign-in")
        }
        let name = newFolderName.trimmingCharacters(in: .whitespaces)
        let operations = operations
        takenName = nil
        run(.makingFolder(name)) {
            do {
                let created = try await operations.make(listing.path.last, name)
                self.isNaming = false
                self.newFolderName = ""
                self.selected = created
                return Listing(
                    path: listing.path,
                    folders: listing.folders + [created]
                )
            }
            catch BridgeError.Diagnostic(
                category: .storage(failure: .alreadyExists),
                detail: _
            ) {
                self.takenName = name
                return listing
            }
        }
    }

    /// Offered a failed setup before the step shows it: a folder holding
    /// other files is named in the list, which the person chooses again
    /// from.
    public func place(_ error: any Error) -> Bool {
        guard
            case .Diagnostic(.storage(.ownLocationRequired), _) = error
                as? BridgeError,
            let selected
        else { return false }
        notEmpty = selected
        return true
    }

    public func cancel() {
        task?.cancel()
    }

    /// Run `operation`, which starts as `work` and moves `working` on as
    /// its calls change.
    private func run(
        _ work: Work,
        _ operation: @escaping @MainActor () async throws -> Listing
    ) {
        error = nil
        notEmpty = nil
        working = work
        task = Task {
            defer {
                task = nil
                working = nil
            }
            do { listing = try await operation() }
            catch is CancellationError {
                logger.debug("folder setup step cancelled")
            }
            catch {
                logger.error(
                    "folder setup step failed: \(String(reflecting: error))"
                )
                self.error = DisplayError(error)
            }
        }
    }
}
