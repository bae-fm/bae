#if BAE_OAUTH_PROVIDERS
    import BaeKit
    import SwiftUI

    /// What a provider's picker says: the line over the list and the empty
    /// list's line. Only these and the provider's calls differ
    /// between Dropbox and OneDrive; the picker draws both the same.
    struct MacFolderPickerWords {
        let heading: LocalizedStringResource
        let empty: LocalizedStringResource
        /// The line when the name given is already taken, for that name.
        let nameTaken: LocalizedStringResource
    }

    extension StorageFolderBrowser.Provider {
        var words: MacFolderPickerWords {
            switch self {
            case .dropbox:
                MacFolderPickerWords(
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
                    nameTaken: LocalizedStringResource(
                        "A folder with this name is already in your Dropbox. Choose another name.",
                        comment:
                            "Dropbox setup, under the new folder name field, when the person named a folder that is already at the top of their Dropbox."
                    )
                )
            case .oneDrive:
                MacFolderPickerWords(
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
                    nameTaken: LocalizedStringResource(
                        "A folder with this name is already here. Choose another name.",
                        comment:
                            "OneDrive setup, under the new folder name field, when the person named a folder that is already in the folder shown."
                    )
                )
            }
        }
    }

    /// The folder list Dropbox and OneDrive setup share: the line over it,
    /// where the person is when folders open into others, the folders to
    /// choose from, the name field for a new one, and the folder setup found
    /// holding other files. New folder… and Look
    /// again are the sheet's, in its footer.
    struct MacFolderPicker: View {
        let browser: StorageFolderBrowser
        let listing: StorageFolderBrowser.Listing

        var body: some View {
            @Bindable
            var browser = browser
            let words = browser.provider.words
            VStack(
                alignment: .leading,
                spacing: ThemeSpace.related + ThemeSpace.line
            ) {
                Text(words.heading)
                    .themeText(.body)
                    .foregroundStyle(Theme.textSecondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if browser.nests {
                    HStack {
                        trail
                        Spacer()
                        if browser.canOpenParent {
                            MacButton("Up", kind: .borderless) {
                                browser.openParent()
                            }
                            .neutral()
                            .disabled(browser.isWorking || browser.isNaming)
                        }
                    }
                }
                MacBox(empty: words.empty) {
                    ForEach(listing.folders, id: \.location) { folder in
                        row(folder)
                    }
                    if browser.isNaming {
                        MacFieldRow(aligningText: {
                            BoxedField(
                                text: $browser.newFolderName,
                                focusesOnAppear: true
                            )
                            .accessibilityLabel(Text("Folder name"))
                            .onSubmit { browser.createFolder() }
                            .onExitCommand { browser.stopNaming() }
                            MacButton("Create", kind: .fieldAction) {
                                browser.createFolder()
                            }
                            .disabled(!browser.canCreateFolder)
                        })
                    }
                }
                if browser.takenName != nil {
                    MacNotice(
                        title: String(localized: words.nameTaken),
                        advice: nil,
                        error: nil
                    )
                }
                if browser.notEmpty != nil {
                    MacNotice(
                        title: String(
                            localized:
                                "This folder is not empty. Choose an empty folder, or make a new one."
                        ),
                        advice: nil,
                        error: nil
                    )
                }
            }
        }

        /// Where the person is: the folders above in secondary text, the one
        /// open in primary.
        private var trail: Text {
            let names = browser.pathNames
            let above = names.dropLast().map { "\($0) › " }.joined()
            return Text(verbatim: above).foregroundStyle(Theme.textSecondary)
                + Text(verbatim: names.last ?? "")
                .foregroundStyle(Theme.textPrimary)
        }

        /// A folder row: tapping chooses it, and where folders open into
        /// others its chevron opens it.
        @ViewBuilder
        private func row(_ folder: BridgeStorageFolder) -> some View {
            let chosen = browser.selected == folder
            let choose = { browser.select(folder) }
            let open = { browser.open(folder) }
            let row = MacRow {
                if browser.nests {
                    Button(action: open) {
                        Image(systemName: "chevron.forward")
                            .themeIcon(.small)
                    }
                    .buttonStyle(.accentBorderless)
                    .disabled(browser.isWorking)
                }
            } label: {
                Text(verbatim: folder.name)
            }
            .pressing(
                browser.nests
                    ? .label(showsChevron: false, action: choose)
                    : nil
            )
            .selected(chosen)
            .contentShape(Rectangle())
            .onTapGesture(perform: choose)

            if browser.nests {
                row.accessibilityRepresentation {
                    Button(action: choose) { Text(verbatim: folder.name) }
                        .accessibilityAddTraits(chosen ? .isSelected : [])
                        .accessibilityActions {
                            if !browser.isWorking {
                                Button("Open folder", action: open)
                            }
                        }
                }
            }
            else {
                row.accessibilityAddTraits(chosen ? .isSelected : [])
            }
        }
    }
#endif
