import BaeKit
import SwiftUI

/// What a selection of several folders can be told to do: one card, centered in
/// the pane, whose rows are the actions the selection offers.
struct MacImportBulk: View {
    @Environment(ImportSelection.self)
    private var selection
    @Environment(ConfigStore.self)
    private var configStore
    @Binding
    var importDestination: BridgeImportDestination
    /// Run one of the selection's actions; the caller confirms any that
    /// replaces the person's choices.
    let onPerform: (ImportCandidateActionOffer) -> Void

    var body: some View {
        // A scroll view gives its content no height, so the pane's height is
        // the minimum: short content centers and tall content scrolls.
        GeometryReader { pane in
            ScrollView {
                VStack(spacing: ThemeSpace.group) {
                    ImportCandidateBulkSelectionCard(
                        selectedCount: Int(selection.summary.count),
                        offers: ImportCandidateActionOffer.selection(
                            selection.summary
                        ),
                        isRunning: selection.isRunning,
                        showsStorageChoices: configStore.syncs,
                        importDestination: $importDestination,
                        onPerform: onPerform
                    )
                    if selection.isRunning {
                        HStack(alignment: .bottom, spacing: ThemeSpace.group) {
                            if let running = selection.running {
                                SelectionActionProgressLine(
                                    action: running.action,
                                    progress: running.progress
                                )
                            }
                            else {
                                Spacer(minLength: 0)
                            }
                            MacButton("Cancel") { selection.cancel() }
                        }
                    }
                }
                .frame(width: ImportCandidateBulkSelectionCard.width)
                .frame(maxWidth: .infinity, minHeight: pane.size.height)
            }
        }
    }
}

/// The selection's actions as one card: the selected count, then the actions
/// in groups, each with how many selected folders it applies to.
struct ImportCandidateBulkSelectionCard: View {
    /// The width the pane centers the card at.
    static let width: CGFloat = 440
    /// The inset from the card's edge to its content.
    static let padding = ThemeSpace.section

    let selectedCount: Int
    let offers: [ImportCandidateActionOffer]
    let isRunning: Bool
    /// Whether the library syncs, which is what gives Import its
    /// destination choice.
    let showsStorageChoices: Bool
    @Binding
    var importDestination: BridgeImportDestination
    let onPerform: (ImportCandidateActionOffer) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            VStack(alignment: .leading, spacing: ThemeSpace.group) {
                Text("\(selectedCount) selected")
                    .themeText(.title)
                Text("Each action applies only to eligible selected folders.")
                    .themeText(.body)
                    .foregroundStyle(Theme.macTextSecondary)
            }
            groupedList
        }
        .padding(Self.padding)
        .frame(width: Self.width)
        .card()
    }

    private var groupedList: some View {
        let groups = drawnGroups
        return VStack(spacing: 0) {
            ForEach(groups) { group in
                if group != groups.first {
                    MacHairline()
                }
                if let title = group.title {
                    MacEyebrow(title)
                        .padding(.top, ThemeSpace.related)
                        .padding(
                            .horizontal,
                            ImportBulkActionRowMetrics.horizontal
                        )
                        .padding(.bottom, ThemeSpace.line)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                ForEach(rows(in: group)) { offer in
                    MacBulkActionRow(
                        offer: offer,
                        importDestination: offer.action == .import
                            && showsStorageChoices ? $importDestination : nil
                    ) { onPerform(offer) }
                    // A running action leaves the rows as they are, and takes
                    // no second one.
                    .allowsHitTesting(!isRunning)
                    .disabled(!offer.enabled)
                }
            }
        }
        .background(Theme.hover)
        .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.card))
        .overlay {
            RoundedRectangle(cornerRadius: ThemeRadius.card)
                .strokeBorder(Theme.hairline, lineWidth: 1)
        }
    }

    /// The groups that have at least one row.
    var drawnGroups: [ImportBulkActionGroup] {
        ImportBulkActionGroup.allCases.filter { !rows(in: $0).isEmpty }
    }

    /// The group's rows, in the order the selection offers them.
    func rows(in group: ImportBulkActionGroup) -> [ImportCandidateActionOffer] {
        offers.filter { group.actions.contains($0.action) }
    }
}

extension ImportCandidateActionOffer {
    /// Whether the action gets folders into the library; its row gets the
    /// accent and a heavier name.
    var isConstructive: Bool { action == .import || action == .combine }
}

/// The groups a selection's actions are listed in, in the order both the card
/// and a row's menu draw them.
enum ImportBulkActionGroup: CaseIterable, Identifiable {
    case importing, metadata, placement, folder

    var id: Self { self }

    /// The label above the group; placement and folder have none, since their
    /// rows say what they do.
    var title: LocalizedStringKey? {
        switch self {
        case .importing: "Import"
        case .metadata: "Metadata"
        case .placement, .folder: nil
        }
    }

    var actions: [BridgeCandidateAction] {
        switch self {
        case .importing: [.import, .cancelImport, .combine, .separate]
        case .metadata:
            [
                .identify, .cancelIdentification, .retryIdentification,
                .resetToFileMetadata, .clearMetadata,
            ]
        case .placement: [.skip, .restore]
        case .folder: [.revealFolder]
        }
    }
}

/// The rows' geometry, shared by the rows themselves and the group titles.
enum ImportBulkActionRowMetrics {
    static let horizontal = ThemeSpace.group
    static let vertical = ThemeSpace.related
    static let spacing = ThemeSpace.group
    static let icon = ThemeIcon.medium
    /// Fixed, so every row's label starts at the same place whatever its
    /// symbol measures.
    static let iconWidth = icon.size
}
