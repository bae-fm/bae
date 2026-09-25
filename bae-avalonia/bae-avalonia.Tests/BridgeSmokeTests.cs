using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.Tests;

// The skeleton's end-to-end check: the generated bindings load the native bridge
// library, build a host, and make the same call the window makes. The host is
// rooted at a fresh temporary home, so the call reads an empty bae directory and
// never the user's.
public sealed class BridgeSmokeTests : IDisposable
{
    private readonly string _home = Directory.CreateTempSubdirectory("bae-avalonia-smoke-").FullName;

    [Fact]
    public void AnEmptyHomeHasNoLibraries()
    {
        var diagnostics = NativeBae.ConfigureDiagnostics(new BridgeDiagnosticsConfig.Disabled());
        var host = NativeBae.CreateHost(diagnostics, new BridgeAppDir(_home));

        Assert.Equal(0, NativeBae.LibraryCount(host));
    }

    public void Dispose() => Directory.Delete(_home, recursive: true);
}
