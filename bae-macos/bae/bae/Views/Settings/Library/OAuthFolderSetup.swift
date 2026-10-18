#if BAE_OAUTH_PROVIDERS
    import BaeKit
    import SwiftUI
    import os.log

    private let logger = Logger.bae("OAuthFolderSetup")

    /// Dropbox or OneDrive setup: sign in through the browser, then choose the
    /// folder the library lives in — a Dropbox shared folder, so the household
    /// can be invited, or any OneDrive folder, which can be made here.
    struct OAuthFolderSetup: View {
        enum Provider {
            case dropbox
            case oneDrive

            var cloudProvider: BridgeCloudProvider {
                switch self {
                case .dropbox: .dropbox
                case .oneDrive: .oneDrive
                }
            }
        }

        let provider: Provider
        let onConnected: (BridgeStorageSetup) -> Void

        @Environment(CloudSyncSetup.self)
        private var cloudSyncSetup

        /// The folders signed in so far: Dropbox's shared folders, or the
        /// OneDrive folders from the root down to the one shown.
        @State
        private var browse: FolderBrowse?
        @State
        private var selected: BridgeStorageFolder?
        @State
        private var newFolderName = ""
        @State
        private var task: Task<Void, Never>?
        @State
        private var error: DisplayError?

        private struct FolderBrowse {
            /// OneDrive's path from the root to the folder shown; empty for
            /// Dropbox, whose shared folders are one flat list.
            var path: [BridgeStorageFolder]
            var folders: [BridgeStorageFolder]
        }

        var body: some View {
            SetupStep(
                actionTitle: "Connect",
                ready: target != nil,
                operation: { [cloudSyncSetup] in
                    guard let target else {
                        preconditionFailure("Connect needs a chosen folder")
                    }
                    return try await cloudSyncSetup.connect(target.location)
                },
                onConnected: onConnected
            ) {
                if let browse {
                    folders(browse)
                }
                else {
                    Section {
                        Text(
                            "Opens your browser to authorize bae with \(provider.cloudProvider.displayName)."
                        )
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        Button("Sign In...") { signIn() }
                            .disabled(task != nil)
                    }
                }
                if let error {
                    Section {
                        ErrorDetailDisclosure(error: error)
                    }
                }
            }
            .onDisappear { task?.cancel() }
        }

        /// The folder Connect uses: the selected one, or on OneDrive the one
        /// being shown.
        private var target: BridgeStorageFolder? {
            if let selected { return selected }
            return browse?.path.last
        }

        @ViewBuilder
        private func folders(_ browse: FolderBrowse) -> some View {
            switch provider {
            case .dropbox:
                Section {
                    if browse.folders.isEmpty {
                        Text(
                            "No shared folders. Share a folder in Dropbox, then look again."
                        )
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                    }
                    Picker("Shared folder", selection: $selected) {
                        ForEach(browse.folders, id: \.location) { folder in
                            Text(folder.name).tag(Optional(folder))
                        }
                    }
                    .pickerStyle(.inline)
                    Button("Look Again") { loadDropboxFolders() }
                        .disabled(task != nil)
                }
            case .oneDrive:
                Section {
                    LabeledContent(
                        "Folder",
                        value: browse.path.last?.name ?? ""
                    )
                    if browse.path.count > 1 {
                        Button("Up") { openParent() }
                            .disabled(task != nil)
                    }
                }
                Section("Folders") {
                    ForEach(browse.folders, id: \.location) { folder in
                        Button(folder.name) { open(folder) }
                            .buttonStyle(.borderless)
                            .disabled(task != nil)
                    }
                    HStack {
                        TextField("New folder", text: $newFolderName)
                        Button("Create") { createFolder() }
                            .disabled(
                                task != nil
                                    || newFolderName.trimmingCharacters(
                                        in: .whitespaces
                                    )
                                    .isEmpty
                            )
                    }
                }
            }
        }

        // MARK: - Actions

        private func signIn() {
            let access: BridgeOAuthAccess =
                switch provider {
                case .dropbox: .dropbox(access: .fullDropbox)
                case .oneDrive: .oneDrive
                }
            run {
                try await cloudSyncSetup.authenticate(access)
                switch provider {
                case .dropbox:
                    browse = FolderBrowse(
                        path: [],
                        folders: try await cloudSyncSetup.dropboxSharedFolders()
                    )
                case .oneDrive:
                    let root = try await cloudSyncSetup.oneDriveRoot()
                    browse = FolderBrowse(
                        path: [root],
                        folders: try await cloudSyncSetup.oneDriveFolders(
                            root.location
                        )
                    )
                }
            }
        }

        private func loadDropboxFolders() {
            run {
                browse = FolderBrowse(
                    path: [],
                    folders: try await cloudSyncSetup.dropboxSharedFolders()
                )
                selected = nil
            }
        }

        private func open(_ folder: BridgeStorageFolder) {
            guard let path = browse?.path else {
                preconditionFailure("folders show only after sign-in")
            }
            run {
                browse = FolderBrowse(
                    path: path + [folder],
                    folders: try await cloudSyncSetup.oneDriveFolders(
                        folder.location
                    )
                )
            }
        }

        private func openParent() {
            guard let path = browse?.path, path.count > 1 else {
                preconditionFailure("Up shows only below the root")
            }
            let parentPath = Array(path.dropLast())
            guard let parent = parentPath.last else {
                preconditionFailure("a path below the root has a parent")
            }
            run {
                browse = FolderBrowse(
                    path: parentPath,
                    folders: try await cloudSyncSetup.oneDriveFolders(
                        parent.location
                    )
                )
            }
        }

        private func createFolder() {
            guard let path = browse?.path, let parent = path.last else {
                preconditionFailure("folders are made only inside a folder")
            }
            let name = newFolderName.trimmingCharacters(in: .whitespaces)
            run {
                let created = try await cloudSyncSetup.createOneDriveFolder(
                    parent.location,
                    name
                )
                newFolderName = ""
                browse = FolderBrowse(
                    path: path + [created],
                    folders: try await cloudSyncSetup.oneDriveFolders(
                        created.location
                    )
                )
            }
        }

        private func run(_ operation: @escaping () async throws -> Void) {
            error = nil
            task = Task {
                defer { task = nil }
                do { try await operation() }
                catch is CancellationError {
                    logger.debug("folder setup step cancelled")
                }
                catch {
                    logger.error(
                        "folder setup step failed: \(error.localizedDescription)"
                    )
                    self.error = DisplayError(error)
                }
            }
        }
    }
#endif
