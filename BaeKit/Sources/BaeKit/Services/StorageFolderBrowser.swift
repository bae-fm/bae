import Foundation
import Observation
import os.log

private let logger = Logger.bae("StorageFolderBrowser")

/// Dropbox or OneDrive setup's state: sign in, then choose the folder the
/// library lives in — a Dropbox shared folder, so the household can be
/// invited, or any OneDrive folder, which can be made here. The provider calls
/// are the app's, since only an edition with OAuth providers has them.
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
        let dropboxSharedFolders:
            @Sendable () async throws -> [BridgeStorageFolder]
        let oneDriveRoot: @Sendable () async throws -> BridgeStorageFolder
        let oneDriveFolders:
            @Sendable (BridgeStorageLocation) async throws ->
                [BridgeStorageFolder]
        let createOneDriveFolder:
            @Sendable (BridgeStorageLocation, String) async throws ->
                BridgeStorageFolder

        public init(
            signIn:
                @escaping @Sendable (BridgeCloudProvider) async throws -> Void,
            dropboxSharedFolders:
                @escaping @Sendable () async throws -> [BridgeStorageFolder],
            oneDriveRoot:
                @escaping @Sendable () async throws -> BridgeStorageFolder,
            oneDriveFolders:
                @escaping @Sendable (BridgeStorageLocation) async throws ->
                [BridgeStorageFolder],
            createOneDriveFolder:
                @escaping @Sendable (BridgeStorageLocation, String) async throws
                -> BridgeStorageFolder
        ) {
            self.signIn = signIn
            self.dropboxSharedFolders = dropboxSharedFolders
            self.oneDriveRoot = oneDriveRoot
            self.oneDriveFolders = oneDriveFolders
            self.createOneDriveFolder = createOneDriveFolder
        }
    }

    /// The folders signed in so far: Dropbox's shared folders, or the OneDrive
    /// folders inside the one shown.
    public struct Listing {
        /// OneDrive's path from the root to the folder shown; empty for
        /// Dropbox, whose shared folders are one flat list.
        public let path: [BridgeStorageFolder]
        public let folders: [BridgeStorageFolder]
    }

    public let provider: Provider
    /// Nil until sign-in lists the first folders.
    public private(set) var listing: Listing?
    /// The Dropbox shared folder chosen from the list.
    public var selected: BridgeStorageFolder?
    public var newFolderName = ""
    public private(set) var isWorking = false
    public private(set) var error: DisplayError?

    @ObservationIgnored
    private let operations: Operations
    @ObservationIgnored
    private var task: Task<Void, Never>?

    public init(provider: Provider, operations: Operations) {
        self.provider = provider
        self.operations = operations
    }

    /// The folder Connect uses: the selected one, or on OneDrive the one being
    /// shown.
    public var target: BridgeStorageFolder? {
        if let selected { return selected }
        return listing?.path.last
    }

    public var canOpenParent: Bool { (listing?.path.count ?? 0) > 1 }

    public var canCreateFolder: Bool {
        !isWorking
            && !newFolderName.trimmingCharacters(in: .whitespaces).isEmpty
    }

    /// Sign in through the bridge's presenter, then list the first folders.
    public func signIn() {
        let operations = operations
        let provider = provider
        run {
            try await operations.signIn(provider.cloudProvider)
            switch provider {
            case .dropbox:
                return Listing(
                    path: [],
                    folders: try await operations.dropboxSharedFolders()
                )
            case .oneDrive:
                let root = try await operations.oneDriveRoot()
                return Listing(
                    path: [root],
                    folders: try await operations.oneDriveFolders(root.location)
                )
            }
        }
    }

    /// List Dropbox's shared folders again, after the person shared one.
    public func lookAgain() {
        let operations = operations
        selected = nil
        run {
            Listing(
                path: [],
                folders: try await operations.dropboxSharedFolders()
            )
        }
    }

    public func open(_ folder: BridgeStorageFolder) {
        guard let path = listing?.path else {
            preconditionFailure("folders show only after sign-in")
        }
        let operations = operations
        run {
            Listing(
                path: path + [folder],
                folders: try await operations.oneDriveFolders(folder.location)
            )
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
        let operations = operations
        run {
            Listing(
                path: parentPath,
                folders: try await operations.oneDriveFolders(parent.location)
            )
        }
    }

    /// Make a folder inside the one shown, and show it.
    public func createFolder() {
        guard let path = listing?.path, let parent = path.last else {
            preconditionFailure("folders are made only inside a folder")
        }
        let name = newFolderName.trimmingCharacters(in: .whitespaces)
        let operations = operations
        run {
            let created = try await operations.createOneDriveFolder(
                parent.location,
                name
            )
            self.newFolderName = ""
            return Listing(
                path: path + [created],
                folders: try await operations.oneDriveFolders(created.location)
            )
        }
    }

    public func cancel() {
        task?.cancel()
    }

    private func run(
        _ operation: @escaping @MainActor () async throws -> Listing
    ) {
        error = nil
        isWorking = true
        task = Task {
            defer {
                task = nil
                isWorking = false
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
