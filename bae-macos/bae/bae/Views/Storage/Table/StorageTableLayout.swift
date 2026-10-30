import Foundation
import OSLog

private let logger = Logger.bae("StorageTableLayout")

/// How the Storage Manager's table lays out its columns: their order, each
/// one's width, and which are hidden. A view preference, kept in the app's
/// defaults as JSON (`@AppStorage` takes it through its raw value); the table
/// applies it and writes back what the person changes.
struct StorageTableLayout: Equatable {
    struct Column: Equatable, Codable {
        let column: StorageTableColumn
        var width: CGFloat
        var hidden: Bool
    }

    /// Every column once, in the order the table shows them.
    private(set) var columns: [Column]

    /// Every column in its declared order at its standard width, shown.
    static let standard = StorageTableLayout(
        columns: StorageTableColumn.allCases.map {
            Column(column: $0, width: $0.width, hidden: false)
        }
    )

    private init(columns: [Column]) {
        self.columns = columns
    }

    /// A layout of `columns`, which must hold every column once: `nil`
    /// otherwise.
    init?(holding columns: [Column]) {
        guard
            columns.count == StorageTableColumn.allCases.count,
            Set(columns.map(\.column)) == Set(StorageTableColumn.allCases)
        else {
            return nil
        }
        self.columns = columns
    }

    func isHidden(_ column: StorageTableColumn) -> Bool {
        columns.contains { $0.column == column && $0.hidden }
    }

    /// The same layout with `column` shown or hidden. The release column
    /// always shows.
    func setting(_ column: StorageTableColumn, hidden: Bool) -> Self {
        guard column.hideable || !hidden else { return self }
        var next = self
        next.columns = columns.map {
            $0.column == column
                ? Column(column: $0.column, width: $0.width, hidden: hidden)
                : $0
        }
        return next
    }
}

extension StorageTableLayout: RawRepresentable {
    /// The layout as its stored JSON reads, or `nil` for a value that isn't a
    /// layout of every column, which leaves the standard layout in place.
    init?(rawValue: String) {
        let columns: [Column]
        do {
            columns = try JSONDecoder()
                .decode(
                    [Column].self,
                    from: Data(rawValue.utf8)
                )
        }
        catch {
            logger.error(
                "Stored storage column layout is unreadable: \(String(reflecting: error))"
            )
            return nil
        }
        guard let layout = Self(holding: columns) else {
            logger.error(
                "Stored storage column layout doesn't hold every column once"
            )
            return nil
        }
        self = layout
    }

    var rawValue: String {
        let data: Data
        do {
            data = try JSONEncoder().encode(columns)
        }
        catch {
            preconditionFailure(
                "A storage column layout always encodes: \(error)"
            )
        }
        guard let json = String(bytes: data, encoding: .utf8) else {
            preconditionFailure("JSONEncoder wrote JSON that isn't UTF-8")
        }
        return json
    }
}
