import AppKit
import BaeKit
import SwiftUI

/// A release's files in its Storage sheet. The table draws the files in the
/// order core returns them and sends column sorts back to the sheet.
struct MacReleaseFileTable: NSViewRepresentable {
    let files: [BridgeFile]
    @Binding
    var sortOrder: [KeyPathComparator<BridgeFile>]

    @Environment(\.surfaceTone)
    private var tone
    @Environment(\.accentChoice)
    private var accent

    private var appearance: HostedAppearance {
        HostedAppearance(tone: tone, accent: accent)
    }

    func makeCoordinator() -> Coordinator {
        Coordinator($sortOrder, appearance: appearance)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let table = context.coordinator.table
        table.style = .inset
        // The sheet supplies the surface behind the rows and their stripes.
        table.backgroundColor = .clear
        table.rowHeight = 24
        table.intercellSpacing = NSSize(width: 17, height: 0)
        table.columnAutoresizingStyle = .uniformColumnAutoresizingStyle
        table.allowsColumnResizing = true
        table.allowsColumnReordering = false
        table.dataSource = context.coordinator
        table.delegate = context.coordinator
        for column in Column.allCases {
            table.addTableColumn(column.makeTableColumn())
        }
        let scrollView = NSScrollView()
        scrollView.documentView = table
        scrollView.drawsBackground = false
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.autohidesScrollers = true
        return scrollView
    }

    func updateNSView(_: NSScrollView, context: Context) {
        context.coordinator.update(
            files: files,
            sortOrder: $sortOrder,
            appearance: appearance
        )
    }

    private enum Column: String, CaseIterable {
        case name, audio, size, kind

        private var identifier: NSUserInterfaceItemIdentifier {
            NSUserInterfaceItemIdentifier(rawValue)
        }

        private var title: String {
            switch self {
            case .name: String(localized: "Name")
            case .audio: localizedCoreString("core.audio.label")
            case .size: String(localized: "Size")
            case .kind: String(localized: "Kind")
            }
        }

        var comparator: KeyPathComparator<BridgeFile>? {
            switch self {
            case .name:
                KeyPathComparator(
                    \.originalFilename,
                    comparator: .localizedStandard
                )
            case .audio: nil
            case .size: KeyPathComparator(\.fileSize)
            case .kind: KeyPathComparator(\.contentType)
            }
        }

        @MainActor
        func makeTableColumn() -> NSTableColumn {
            let column = NSTableColumn(identifier: identifier)
            column.title = title
            column.isEditable = false
            switch self {
            case .name, .audio: break
            case .size:
                column.minWidth = 70
                column.width = 80
            case .kind:
                column.minWidth = 90
                column.width = 130
            }
            if comparator != nil {
                column.sortDescriptorPrototype = NSSortDescriptor(
                    key: rawValue,
                    ascending: true
                )
            }
            return column
        }

        @MainActor
        @ViewBuilder
        func cell(for file: BridgeFile) -> some View {
            switch self {
            case .name:
                Text(file.originalFilename)
                    .cutLine(.fileName)
                    .foregroundStyle(Theme.textPrimary)
            case .audio:
                if let format = file.audioFormat {
                    Text(format.text)
                        .foregroundStyle(Theme.macTextSecondary)
                        .lineLimit(1)
                }
            case .size:
                Text(file.fileSizeText)
                    .monospacedDigit()
                    .foregroundStyle(Theme.macTextSecondary)
            case .kind:
                Text(file.contentType)
                    .foregroundStyle(Theme.textTertiary)
                    .lineLimit(1)
            }
        }
    }

    @MainActor
    final class Coordinator: NSObject, NSTableViewDataSource,
        NSTableViewDelegate
    {
        let table = NSTableView()
        private var sortOrder: Binding<[KeyPathComparator<BridgeFile>]>
        private var appearance: HostedAppearance
        private var files: [BridgeFile] = []

        init(
            _ sortOrder: Binding<[KeyPathComparator<BridgeFile>]>,
            appearance: HostedAppearance
        ) {
            self.sortOrder = sortOrder
            self.appearance = appearance
        }

        func update(
            files: [BridgeFile],
            sortOrder: Binding<[KeyPathComparator<BridgeFile>]>,
            appearance: HostedAppearance
        ) {
            self.sortOrder = sortOrder
            let descriptors = sortOrder.wrappedValue.compactMap { sort in
                Column.allCases.first { $0.comparator?.keyPath == sort.keyPath }
                    .map {
                        NSSortDescriptor(
                            key: $0.rawValue,
                            ascending: sort.order == .forward
                        )
                    }
            }
            if table.sortDescriptors != descriptors {
                table.sortDescriptors = descriptors
            }
            if files != self.files || appearance != self.appearance {
                self.files = files
                self.appearance = appearance
                table.reloadData()
            }
        }

        func numberOfRows(in table: NSTableView) -> Int { files.count }

        func tableView(_: NSTableView, shouldSelectRow _: Int) -> Bool {
            false
        }

        func tableView(_: NSTableView, rowViewForRow row: Int)
            -> NSTableRowView?
        {
            BrowsedTableRowView(
                accent: appearance.accent,
                alternate: row % 2 == 1
            )
        }

        func tableView(
            _ table: NSTableView,
            viewFor tableColumn: NSTableColumn?,
            row: Int
        ) -> NSView? {
            guard let identifier = tableColumn?.identifier,
                let column = Column(rawValue: identifier.rawValue)
            else {
                preconditionFailure(
                    "The release file table has an unknown column"
                )
            }
            let cell = HostingTableCell.dequeue(
                from: table,
                identifier: identifier,
                owner: self
            )
            cell.host(
                column.cell(for: files[row])
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .hostedAppearance(appearance)
            )
            return cell
        }

        func tableView(
            _ table: NSTableView,
            sortDescriptorsDidChange _: [NSSortDescriptor]
        ) {
            let order = table.sortDescriptors.compactMap {
                descriptor -> KeyPathComparator<BridgeFile>? in
                guard let key = descriptor.key,
                    var comparator = Column(rawValue: key)?.comparator
                else { return nil }
                comparator.order = descriptor.ascending ? .forward : .reverse
                return comparator
            }
            if sortOrder.wrappedValue != order {
                sortOrder.wrappedValue = order
            }
        }
    }
}
