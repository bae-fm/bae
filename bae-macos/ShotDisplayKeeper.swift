import AppKit
import Darwin
import OSLog

/// Owns the virtual display until the last capture drops its shared lock.
@main
struct ShotDisplayKeeper {
    @MainActor
    static func main() {
        let app = NSApplication.shared
        guard app.setActivationPolicy(.prohibited) else {
            let error =
                "AppKit refused to disable activation for the shot display keeper."
            Logger.bae("ShotDisplayKeeper").error(error)
            FileHandle.standardOutput.write(Data("\(error)\n".utf8))
            exit(EXIT_FAILURE)
        }
        Task { @MainActor in
            await holdDisplay()
            exit(EXIT_SUCCESS)
        }
        app.run()
    }

    @MainActor
    private static func holdDisplay() async {
        let args = CommandLine.arguments
        guard args.count == 4, let startup = Int32(args[2]),
            let owner = Int32(args[3])
        else {
            FileHandle.standardOutput.write(
                Data(
                    "The shot display keeper needs a directory and two lock descriptors.\n"
                        .utf8
                )
            )
            exit(EXIT_FAILURE)
        }
        let root = URL(fileURLWithPath: args[1], isDirectory: true)
        let output = FileHandle.standardOutput
        do {
            let active = try FileHandle(
                forReadingFrom: root.appendingPathComponent("active")
            )
            let id: CGDirectDisplayID
            do {
                let display = try await ShotDisplay.add(
                    size: ShotDisplay.sharedSize
                )
                id = display.id
                let identity = Data("\(id)\n".utf8)
                try identity.write(
                    to: root.appendingPathComponent("display-id"),
                    options: .atomic
                )
                // Publish the identity before letting another capture join.
                close(startup)
                try output.write(contentsOf: identity)
                try output.close()
                try await waitForCaptures(active)
                withExtendedLifetime(display) {}
            }
            // Keep the exclusive activity lock until AppKit sees the display
            // go away, so the next owner does not change the layout mid-shot.
            try await Wait.until(notices: [
                Wait.Notice(
                    NSApplication.didChangeScreenParametersNotification,
                    of: nil
                )
            ]) {
                ShotDisplay.screen(for: id) == nil
            }
            close(owner)
            try active.close()
        }
        catch {
            Logger.bae("ShotDisplayKeeper").error(String(describing: error))
            if output.fileDescriptor >= 0 {
                try? output.write(contentsOf: Data("\(error)\n".utf8))
            }
            exit(EXIT_FAILURE)
        }
    }

    private static func waitForCaptures(_ active: FileHandle) async throws {
        let descriptor = active.fileDescriptor
        try await withCheckedThrowingContinuation {
            (waiting: CheckedContinuation<Void, any Error>) in
            DispatchQueue.global()
                .async {
                    while flock(descriptor, LOCK_EX) != 0 {
                        if errno == EINTR { continue }
                        waiting.resume(
                            throwing: NSError(
                                domain: NSPOSIXErrorDomain,
                                code: Int(errno)
                            )
                        )
                        return
                    }
                    waiting.resume()
                }
        }
    }
}
