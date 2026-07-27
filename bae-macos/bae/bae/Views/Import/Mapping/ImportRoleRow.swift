import BaeKit
import Foundation

/// One row of the mapping pane's roles table: a file, or the group row standing
/// in for a whole directory core decided to collapse.
///
/// Which directories collapse is core's decision (`collapsedDirectories`); this
/// only places the group row where its first file would have gone and drops the
/// rest, so every file in the folder is accounted for exactly once.
enum ImportRoleRow: Identifiable {
    case file(BridgeCandidateFile)
    case directory(BridgeCollapsedDirectory)

    var id: String {
        switch self {
        case .file(let file): "file:\(file.file.name)"
        case .directory(let dir): "dir:\(dir.dirPrefix)"
        }
    }

    /// Walk `files` in release-relative path order. A file whose `dirPrefix`
    /// names a collapsed directory is stood for by that directory's group row,
    /// emitted once at the position of the first such file.
    static func rows(of files: BridgeCandidateFiles) -> [ImportRoleRow] {
        let collapsed = Dictionary(
            files.collapsedDirectories.map { ($0.dirPrefix, $0) },
            uniquingKeysWith: { first, _ in first }
        )
        var emitted: Set<String> = []
        var rows: [ImportRoleRow] = []
        for file in files.files {
            guard let prefix = file.file.dirPrefix,
                let directory = collapsed[prefix]
            else {
                rows.append(.file(file))
                continue
            }
            if emitted.insert(prefix).inserted {
                rows.append(.directory(directory))
            }
        }
        return rows
    }
}
