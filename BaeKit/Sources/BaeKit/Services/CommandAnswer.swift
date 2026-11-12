/// Run a command nobody waits on, and show why it failed: a cancellation has
/// nothing to show.
func answerCommand(
    _ show: @escaping @MainActor @Sendable (DisplayError) -> Void,
    _ command: @escaping @Sendable () async throws -> Void
) {
    Task { @MainActor in
        do {
            try await command()
        }
        catch {
            if let shown = DisplayError(error) {
                show(shown)
            }
        }
    }
}
