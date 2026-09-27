import BaeKit
import SwiftUI

/// What a selection of several folders can be told to do: one card, centered in
/// the pane, whose rows are the actions the selection offers.
struct ImportCandidateBulkSelectionPane: View {
    @Environment(ImportSelection.self)
    private var selection
    @Environment(ConfigStore.self)
    private var configStore
    @Binding
    var storageCloud: Bool
    @Binding
    var storagePinned: Bool
    /// Run one of the selection's actions; the caller confirms any that
    /// replaces the person's choices.
    let onPerform: (ImportCandidateActionOffer) -> Void

    var body: some View {
        // A scroll view gives its content no height, so the pane's height is
        // the minimum: short content centers and tall content scrolls.
        GeometryReader { pane in
            ScrollView {
                VStack(spacing: ThemeSpace.edge) {
                    ImportCandidateBulkSelectionCard(
                        selectedCount: Int(selection.summary.count),
                        offers: ImportCandidateActionOffer.selection(
                            selection.summary
                        ),
                        isRunning: selection.isRunning,
                        showsStorageChoices: configStore.config.hasCloudHome,
                        storageCloud: $storageCloud,
                        storagePinned: $storagePinned,
                        onPerform: onPerform
                    )
                    if let progress = selection.progress {
                        ProgressView(
                            value: Double(progress.completed),
                            total: Double(progress.total)
                        ) {
                            Text(progress.action.label(count: progress.total))
                        }
                    }
                    if selection.isRunning {
                        Button("Cancel") { selection.cancel() }
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
    /// Whether the library has a cloud home, which is what gives Import its
    /// storage choices.
    let showsStorageChoices: Bool
    @Binding
    var storageCloud: Bool
    @Binding
    var storagePinned: Bool
    let onPerform: (ImportCandidateActionOffer) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.edge) {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                Text("\(selectedCount) selected")
                    .themeText(.title)
                Text("Each action applies only to eligible selected folders.")
                    .themeText(.body)
                    .foregroundStyle(.secondary)
            }
            groupedList
        }
        .padding(Self.padding)
        .frame(width: Self.width)
        .background(Theme.surface)
        .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.card))
        .overlay {
            RoundedRectangle(cornerRadius: ThemeRadius.card)
                .strokeBorder(Theme.hairline, lineWidth: 1)
        }
    }

    private var groupedList: some View {
        let groups = drawnGroups
        return VStack(spacing: 0) {
            ForEach(groups) { group in
                if group != groups.first {
                    Rectangle()
                        .fill(Theme.hairline)
                        .frame(height: 1)
                }
                if let title = group.title {
                    Eyebrow(title)
                        .padding(.top, ThemeSpace.related)
                        .padding(
                            .horizontal,
                            ImportBulkActionRowMetrics.horizontal
                        )
                        .padding(.bottom, ThemeSpace.inline)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                ForEach(rows(in: group)) { offer in
                    BulkActionRow(offer: offer) { onPerform(offer) }
                        .disabled(isRunning || !offer.enabled)
                    if offer.action == .import, showsStorageChoices {
                        storageChoices
                    }
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

    /// The import's storage choices, lined up under the Import row's label.
    private var storageChoices: some View {
        HStack(spacing: ThemeSpace.group) {
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

/// The rows' geometry, shared by the rows themselves and by what lines up under
/// a row's label.
enum ImportBulkActionRowMetrics {
    static let horizontal = ThemeSpace.group
    static let vertical = ThemeSpace.related
    static let spacing = ThemeSpace.group
    static let icon = ThemeIcon.medium
    /// Fixed, so every row's label starts at the same place whatever its
    /// symbol measures.
    static let iconWidth = icon.size
    /// Where a row's label starts, from the group's leading edge.
    static let labelInset = horizontal + iconWidth + spacing
}

/// One row of the grouped list: the action's symbol, its name, and how many of
/// the selected folders it applies to.
private struct BulkActionRow: View {
    let offer: ImportCandidateActionOffer
    let action: () -> Void

    private var isConstructive: Bool { offer.isConstructive }

    var body: some View {
        Button(action: action) {
            HStack(spacing: ImportBulkActionRowMetrics.spacing) {
                Image(systemName: offer.action.symbol)
                    .themeIcon(ImportBulkActionRowMetrics.icon)
                    .foregroundStyle(
                        isConstructive ? Theme.accent : Color.secondary
                    )
                    .frame(width: ImportBulkActionRowMetrics.iconWidth)
                Text(verbatim: offer.action.label)
                    .themeText(isConstructive ? .strong : .body)
                    .foregroundStyle(
                        isConstructive ? Color.primary : Color.secondary
                    )
                    .frame(maxWidth: .infinity, alignment: .leading)
                if let count = offer.count {
                    Text(verbatim: count.formatted())
                        .themeText(.chip)
                        .monospacedDigit()
                        .foregroundStyle(
                            isConstructive ? Theme.accent : Color.secondary
                        )
                        .padding(.vertical, ThemeSpace.line)
                        .padding(.horizontal, ThemeSpace.related)
                        .background(
                            Capsule()
                                .fill(
                                    isConstructive
                                        ? Theme.accentSoft
                                        : Theme.pressed
                                )
                        )
                }
            }
        }
        .buttonStyle(BulkActionRowButtonStyle())
    }
}

/// A full-width row with a hover and press fill; the group clips its corners.
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
                return Theme.pressed
            }
            return isHovered ? Theme.hover : .clear
        }
    }
}
