import BaeKit
import SwiftUI

/// What a selection of several folders can be told to do: one card, centered in
/// the pane, whose rows are the actions the selection offers.
struct ImportCandidateBulkSelectionPane: View {
    @Environment(ImportStore.self)
    private var importStore
    @Environment(UiStore.self)
    private var uiStore
    @Environment(ConfigStore.self)
    private var configStore
    @Binding
    var storageCloud: Bool
    @Binding
    var storagePinned: Bool
    /// Run one of the selection's actions. Whoever runs it asks before one
    /// that replaces a person's choices, as a row's menu does too.
    let onPerform: (ImportCandidateActionOffer) -> Void

    var body: some View {
        // A scroll view proposes its content no height of its own, so the
        // height to center the card in is the pane's, read here and asked for
        // as a floor: shorter content sits at the center, taller content keeps
        // its top edge and scrolls.
        GeometryReader { pane in
            ScrollView {
                VStack(spacing: 16) {
                    let selection = ImportCandidateSelection(
                        importStore: importStore,
                        uiStore: uiStore
                    )
                    ImportCandidateBulkSelectionCard(
                        selectedCount: uiStore.selectedFolderCandidates.count,
                        offers: selection.offers,
                        isRunning: uiStore.candidateActionRun.isRunning,
                        showsStorageChoices: configStore.config.hasCloudHome,
                        storageCloud: $storageCloud,
                        storagePinned: $storagePinned,
                        onPerform: onPerform
                    )
                    if let progress = uiStore.candidateActionRun.progress {
                        ProgressView(
                            value: Double(progress.completed),
                            total: Double(progress.total)
                        ) {
                            Text(progress.action.label(count: progress.total))
                        }
                    }
                    if uiStore.candidateActionRun.isRunning {
                        Button("Cancel") { uiStore.candidateActionRun.cancel() }
                    }
                }
                .frame(width: ImportCandidateBulkSelectionCard.width)
                .frame(maxWidth: .infinity, minHeight: pane.size.height)
            }
        }
    }
}

/// The selection's actions as one card: how many folders are selected, and a
/// grouped list of what they can be told to do — each row an action with the
/// number of selected folders it applies to.
struct ImportCandidateBulkSelectionCard: View {
    /// The width the pane centers the card at.
    static let width: CGFloat = 440
    /// The inset from the card's edge to its content.
    static let padding: CGFloat = 24

    let selectedCount: Int
    let offers: [ImportCandidateActionOffer]
    let isRunning: Bool
    /// Whether the library has a cloud home, which is what gives Import ready
    /// its storage choices.
    let showsStorageChoices: Bool
    @Binding
    var storageCloud: Bool
    @Binding
    var storagePinned: Bool
    let onPerform: (ImportCandidateActionOffer) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 8) {
                Text("\(selectedCount) selected")
                    .font(.system(size: 22, weight: .semibold))
                Text("Each action applies only to eligible selected folders.")
                    .font(.system(size: 12.5))
                    .foregroundStyle(.secondary)
            }
            groupedList
        }
        .padding(Self.padding)
        .frame(width: Self.width)
        .background(Theme.surface)
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay {
            RoundedRectangle(cornerRadius: 12)
                .strokeBorder(Theme.hairline, lineWidth: 1)
        }
    }

    private var groupedList: some View {
        let groups = drawnGroups
        return VStack(spacing: 0) {
            ForEach(groups) { group in
                if group != groups.first {
                    Rectangle()
                        .fill(bulkSelectionGroupRule)
                        .frame(height: 1)
                }
                if let title = group.title {
                    FormEyebrow(text: Text(title), size: 10.5)
                        .padding(.top, 8)
                        .padding(
                            .horizontal,
                            ImportBulkActionRowMetrics.horizontal
                        )
                        .padding(.bottom, 4)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                ForEach(rows(in: group)) { offer in
                    BulkActionRow(offer: offer) { onPerform(offer) }
                        .disabled(isRunning || !offer.enabled)
                    if offer.action == .importReady, showsStorageChoices {
                        storageChoices
                    }
                }
            }
        }
        .background(Color.primary.opacity(0.035))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay {
            RoundedRectangle(cornerRadius: 10)
                .strokeBorder(bulkSelectionGroupRule, lineWidth: 1)
        }
    }

    /// Where the imported files go, asked where the import row is: a sub-row
    /// lined up under that row's label.
    private var storageChoices: some View {
        HStack(spacing: 12) {
            ImportCheckboxToggle("Cloud", isOn: $storageCloud)
            if storageCloud {
                ImportCheckboxToggle("Pinned", isOn: $storagePinned)
            }
        }
        .padding(.leading, ImportBulkActionRowMetrics.labelInset)
        .padding(.trailing, ImportBulkActionRowMetrics.horizontal)
        .padding(.bottom, ImportBulkActionRowMetrics.vertical)
        .frame(maxWidth: .infinity, alignment: .leading)
        .disabled(isRunning)
    }

    /// The groups holding a row. A group whose actions nothing in the
    /// selection offers is not drawn.
    var drawnGroups: [ImportBulkActionGroup] {
        ImportBulkActionGroup.allCases.filter { !rows(in: $0).isEmpty }
    }

    /// The group's rows, in the order the selection offers them.
    func rows(in group: ImportBulkActionGroup) -> [ImportCandidateActionOffer] {
        offers.filter { group.actions.contains($0.action) }
    }
}

extension ImportCandidateActionOffer {
    /// How many of the selected folders the row applies to, absent for an
    /// action over the selection as a whole.
    var count: Int? { action == .combine ? nil : targets.count }

    /// Whether the row gets folders into the library, which is what the
    /// accent and the heavier name mark.
    var isConstructive: Bool { action == .importReady || action == .combine }
}

/// The groups a selection's actions are listed in, in the order both the card
/// and a row's menu draw them: getting the folders in, where their metadata
/// comes from, whether they are in the queue at all, and where they are.
enum ImportBulkActionGroup: CaseIterable, Identifiable {
    case importing, metadata, placement, folder

    var id: Self { self }

    /// The label above the group. The last group carries none — a row that
    /// takes folders out of the queue says that itself.
    var title: LocalizedStringKey? {
        switch self {
        case .importing: "Import"
        case .metadata: "Metadata"
        case .placement, .folder: nil
        }
    }

    var actions: [BridgeCandidateAction] {
        switch self {
        case .importing: [.importReady, .cancelImport, .combine, .separate]
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

/// The rules inside the grouped list — its own edge and the lines between its
/// groups, drawn lighter than the card's border so the group reads as one block.
private let bulkSelectionGroupRule = Color.primary.opacity(0.06)

/// The rows' geometry, shared by the rows themselves and by what lines up under
/// a row's label.
enum ImportBulkActionRowMetrics {
    static let horizontal: CGFloat = 14
    static let vertical: CGFloat = 10
    static let spacing: CGFloat = 11
    /// Fixed, so every row's label starts at the same place whatever its
    /// symbol measures.
    static let iconWidth: CGFloat = 17
    /// Where a row's label starts, from the group's leading edge.
    static let labelInset = horizontal + iconWidth + spacing
}

/// One row of the grouped list: the action's symbol, its name, and how many of
/// the selected folders it applies to.
///
/// A constructive row — one that gets folders into the library — carries the
/// accent and a heavier name; the rest are quieter, so the card states what it
/// is for at a glance.
private struct BulkActionRow: View {
    let offer: ImportCandidateActionOffer
    let action: () -> Void

    private var isConstructive: Bool { offer.isConstructive }

    var body: some View {
        Button(action: action) {
            HStack(spacing: ImportBulkActionRowMetrics.spacing) {
                Image(systemName: offer.action.symbol)
                    .font(.system(size: 15))
                    .foregroundStyle(
                        isConstructive ? Theme.accent : Color.secondary
                    )
                    .frame(width: ImportBulkActionRowMetrics.iconWidth)
                Text(verbatim: offer.action.label)
                    .font(
                        .system(
                            size: 13,
                            weight: isConstructive ? .semibold : .regular
                        )
                    )
                    .foregroundStyle(
                        isConstructive ? Color.primary : Color.secondary
                    )
                    .frame(maxWidth: .infinity, alignment: .leading)
                if let count = offer.count {
                    Text(verbatim: count.formatted())
                        .font(.system(size: 11, weight: .bold))
                        .monospacedDigit()
                        .foregroundStyle(
                            isConstructive ? Theme.accent : Color.secondary
                        )
                        .padding(.vertical, 2)
                        .padding(.horizontal, 8)
                        .background(
                            Capsule()
                                .fill(
                                    isConstructive
                                        ? Theme.accentSoft
                                        : Color.primary.opacity(0.08)
                                )
                        )
                }
            }
        }
        .buttonStyle(BulkActionRowButtonStyle())
    }
}

/// A row of the grouped list: full width, no rounding of its own — the group
/// clips its corners — and a hover wash while the row can be pressed.
private struct BulkActionRowButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Row(configuration: configuration)
    }

    private struct Row: View {
        let configuration: Configuration
        @Environment(\.isEnabled)
        private var isEnabled
        @State
        private var isHovered = false

        var body: some View {
            configuration.label
                .padding(.vertical, ImportBulkActionRowMetrics.vertical)
                .padding(.horizontal, ImportBulkActionRowMetrics.horizontal)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(fill)
                .contentShape(Rectangle())
                .opacity(isEnabled ? 1 : 0.4)
                .onHover { isHovered = $0 }
        }

        private var fill: Color {
            guard isEnabled else { return .clear }
            if configuration.isPressed {
                return Color.primary.opacity(0.09)
            }
            return isHovered ? Color.primary.opacity(0.05) : .clear
        }
    }
}
