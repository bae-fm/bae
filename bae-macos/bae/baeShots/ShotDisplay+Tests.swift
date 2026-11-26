import Foundation

extension ShotDisplay {
    private static var adding: Task<ShotDisplay, any Error>?

    /// The display bae's tests of its shots draw on, added by the first call
    /// and kept until the process ends. One display per process means no
    /// display is created within milliseconds of another's destruction,
    /// which WindowServer can leave behind.
    static func forTests() async throws -> ShotDisplay {
        let task = adding ?? Task { try await add(size: size) }
        adding = task
        return try await task.value
    }
}
