#if DEBUG
using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Static fixtures for the shot-capture scenes — the cross-platform analogue of
// the macOS PreviewData set. Generic placeholder values only: no real
// artist/album/song names, no bridge, keychain, or library access. Compiled only
// in DEBUG builds, alongside the capture harness it feeds.
internal static class PreviewData
{
    // Fixture libraries for the welcome chooser: two on-disk libraries the welcome
    // scene lists as re-openable, plus the create/join/restore actions. Local only
    // (no cloud provider), inactive, no open error — placeholder ids/names, no
    // keychain or on-disk library behind them.
    internal static List<BridgeLibrary> WelcomeLibraries { get; } = new()
    {
        new BridgeLibrary("lib-home", "Home Library", @"C:\Users\Example\Music\Home", null, false, null),
        new BridgeLibrary("lib-studio", "Studio Library", @"C:\Users\Example\Music\Studio", null, false, null),
    };
}
#endif
