import OSLog

extension Logger {
    static func bae(_ category: String) -> Logger {
        Logger(subsystem: "fm.bae.desktop", category: category)
    }
}
