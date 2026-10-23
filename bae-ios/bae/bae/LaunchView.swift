import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("LaunchView")

/// The skeleton's only screen: the sheep, the wordmark, and either the launch
/// step that failed or what one real bridge call returns, so a launch
/// exercises the generated bindings, the native library and the host end to
/// end. The iOS UI itself is not built here.
struct LaunchView: View {
    let launch: AppLaunch

    var body: some View {
        VStack(spacing: 16) {
            Image("SheepIcon")
                .resizable()
                .scaledToFit()
                .frame(width: 96, height: 96)
            Text(verbatim: "bae")
                .font(.largeTitle.weight(.semibold))
            switch launch {
            case .ready(let host):
                LibraryCount(host: host)
            case .failed(let step, let error):
                Text(verbatim: "Launch stopped at \(step.name).")
                Text(verbatim: error)
                    .foregroundStyle(.secondary)
            }
        }
        .multilineTextAlignment(.center)
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// The libraries on this device, read from the host once the screen shows.
private struct LibraryCount: View {
    /// The read's outcome: not back yet, the libraries, or its error.
    private enum Read {
        case reading
        case listed([BridgeLibrary])
        case failed(String)
    }

    let host: BridgeHost

    @State
    private var read = Read.reading

    var body: some View {
        Group {
            switch read {
            case .reading:
                Text(verbatim: "Reading the libraries on this device…")
            case .listed(let libraries):
                Text(verbatim: "\(libraries.count) libraries on this device.")
                ForEach(libraries, id: \.id) { library in
                    Text(verbatim: library.name)
                        .foregroundStyle(.secondary)
                }
            case .failed(let error):
                Text(verbatim: "Could not read the libraries on this device.")
                Text(verbatim: error)
                    .foregroundStyle(.secondary)
            }
        }
        .task {
            do {
                read = .listed(try await host.listLibraries())
            }
            catch {
                logger.error(
                    "Failed to list libraries: \(String(describing: error))"
                )
                read = .failed(String(describing: error))
            }
        }
    }
}
