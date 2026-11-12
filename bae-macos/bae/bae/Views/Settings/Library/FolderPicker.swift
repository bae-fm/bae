#if BAE_OAUTH_PROVIDERS
    import BaeKit
    import SwiftUI

    /// What a provider's picker says: the line over the list and the empty
    /// list's line. Only these and the provider's calls differ
    /// between Dropbox and OneDrive; the picker draws both the same.
    struct FolderPickerWords {
        let heading: LocalizedStringResource
        let empty: LocalizedStringResource
        /// The line when the name given is already taken, for that name.
        let nameTaken: (String) -> LocalizedStringResource
    }

    extension StorageFolderBrowser.Provider {
        var words: FolderPickerWords {
            switch self {
            case .dropbox:
                FolderPickerWords(
                    heading: LocalizedStringResource(
                        "Choose an empty shared folder for this library.",
                        comment:
                            "Dropbox setup's folder list, the line over it."
                    ),
                    empty: LocalizedStringResource(
                        "No shared folders yet",
                        comment:
                            "Dropbox setup's folder list, when the person has no shared folder."
                    ),
                    nameTaken: {
                        LocalizedStringResource(
                            "A folder named “\($0)” is already in your Dropbox. Choose another name.",
                            comment:
                                "Dropbox setup, under the new folder name field, when the person named a folder that is already at the top of their Dropbox; the argument is the name."
                        )
                    }
                )
            case .oneDrive:
                FolderPickerWords(
                    heading: LocalizedStringResource(
                        "Choose an empty folder for this library.",
                        comment:
                            "OneDrive setup's folder list, the line over it."
                    ),
                    empty: LocalizedStringResource(
                        "No folders here yet",
                        comment:
                            "OneDrive setup's folder list, when the folder shown holds no folders."
                    ),
                    nameTaken: {
                        LocalizedStringResource(
                            "A folder named “\($0)” is already here. Choose another name.",
                            comment:
                                "OneDrive setup, under the new folder name field, when the person named a folder that is already in the folder shown; the argument is the name."
                        )
                    }
                )
            }
        }
    }

    /// The folder list Dropbox and OneDrive setup share: the line over it,
    /// where the person is when folders open into others, the folders to
    /// choose from, the name field for a new one, and the folder setup found
    /// holding other files. Its sections go in a form; New folder… and Look
    /// again are the sheet's, in its footer.
    struct FolderPicker: View {
        let browser: StorageFolderBrowser

        @Environment(\.accentChoice)
        private var accent

        var body: some View {
            @Bindable
            var browser = browser
            let words = browser.provider.words
            Section {
                Text(words.heading)
                    .themeText(.body)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if browser.nests {
                    HStack {
                        Text(
                            browser.pathNames.dropFirst()
                                .reduce(
                                    browser.pathNames.first ?? ""
                                ) { path, name in
                                    String(localized: "\(path) › \(name)")
                                }
                        )
                        .themeText(.body)
                        Spacer()
                        if browser.canOpenParent {
                            Button("Up") { browser.openParent() }
                                .buttonStyle(.borderless)
                                .disabled(browser.isWorking)
                        }
                    }
                }
            }
            Section {
                if let listing = browser.listing {
                    if listing.folders.isEmpty && !browser.isNaming {
                        Text(words.empty)
                            .themeText(.body)
                            .foregroundStyle(.secondary)
                    }
                    ForEach(listing.folders, id: \.location) { folder in
                        row(folder)
                    }
                }
                if browser.isNaming {
                    HStack {
                        TextField("Folder name", text: $browser.newFolderName)
                            .labelsHidden()
                            .textFieldStyle(.plain)
                            .onSubmit { browser.createFolder() }
                            .onExitCommand { browser.stopNaming() }
                        Button("Create") { browser.createFolder() }
                            .disabled(!browser.canCreateFolder)
                    }
                }
            }
            if let name = browser.takenName {
                Section {
                    Text(words.nameTaken(name))
                        .themeText(.body)
                        .foregroundStyle(Theme.danger)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            if let taken = browser.notEmpty {
                Section {
                    Text(
                        "“\(taken.name)” isn’t empty. Choose an empty folder, or make a new one."
                    )
                    .themeText(.body)
                    .foregroundStyle(Theme.danger)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }

        /// A folder row: tapping chooses it, and where folders open into
        /// others its chevron opens it.
        private func row(_ folder: BridgeStorageFolder) -> some View {
            let chosen = browser.selected == folder
            return HStack {
                Text(folder.name)
                Spacer()
                if browser.nests {
                    Button {
                        browser.open(folder)
                    } label: {
                        Image(systemName: "chevron.forward")
                    }
                    .buttonStyle(.borderless)
                    .disabled(browser.isWorking)
                }
            }
            .foregroundStyle(chosen ? Theme.onFill : Color.primary)
            .padding(ThemeSpace.fieldInset)
            .background(chosen ? accent.buttonColor : Color.clear)
            .padding(-ThemeSpace.fieldInset)
            .contentShape(Rectangle())
            .onTapGesture { browser.select(folder) }
            .accessibilityAddTraits(chosen ? .isSelected : [])
        }
    }
#endif
