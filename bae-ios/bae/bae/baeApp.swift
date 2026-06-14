import SwiftUI

@main
struct baeApp: App {
  private let oauthLinking: OAuthLinking?
  private let oauthLinkingError: String?
  private let startupError: String?

  init() {
    var loadedOAuthLinking: OAuthLinking?
    var oauthError: String?
    var launchError: String?
    do {
      // App processes on iOS have no $HOME, which bae-core needs to locate its
      // data root (`~/.bae`). Point it at our Application Support container
      // before any library access (discover/restore/initApp) so those don't
      // fail with "could not determine home directory". `set_data_dir` sets
      // HOME, so it must run before `initKeyring`.
      setDataDir(path: try Self.dataDirectory())
      // No `setCaCertDir` on iOS — the TLS stack uses Apple's trust roots.
      initKeyring()
      // Hand Rust the CloudKit driver once. It can't build the driver itself
      // (it needs the platform CloudKit APIs); installing it is idempotent and
      // harmless for libraries that sync elsewhere, so it belongs here at the
      // composition root rather than at each library open.
      setCloudkitDriver(driver: CloudKitService.bae())
    } catch {
      launchError = error.localizedDescription
    }

    if launchError == nil {
      // Register the host's OAuth client creds (if a creds file is bundled) so
      // coven can build authorization URLs and refresh provider tokens during
      // sync. Absent file → cloud providers that need OAuth stay unavailable.
      do {
        loadedOAuthLinking = try OAuthLinking.load()
        try loadedOAuthLinking?.register()
      } catch {
        oauthError = error.localizedDescription
      }
    }
    oauthLinking = loadedOAuthLinking
    oauthLinkingError = oauthError
    startupError = launchError
  }

  var body: some Scene {
    WindowGroup {
      ContentView(
        oauthLinking: oauthLinking,
        oauthLinkingError: oauthLinkingError,
        startupError: startupError
      )
        .tint(Theme.accent)
    }
  }

  /// Absolute path to the app's Application Support directory, created if
  /// absent. bae-core writes its library tree and config under here.
  private static func dataDirectory() throws -> String {
    let fileManager = FileManager.default
    let base = fileManager.urls(
      for: .applicationSupportDirectory,
      in: .userDomainMask
    )[0]
    try fileManager.createDirectory(
      at: base,
      withIntermediateDirectories: true
    )
    return base.path
  }
}
