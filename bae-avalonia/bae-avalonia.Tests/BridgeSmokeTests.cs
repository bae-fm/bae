using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.Tests;

// The skeleton's end-to-end check: the generated bindings load the native bridge
// library, build a host, and make the same call the window makes. The host is
// rooted at a fresh temporary home and keeps its keys in an in-memory keychain,
// so the call reads an empty bae directory and never the user's, and nothing
// reaches the user's keychain.
public sealed class BridgeSmokeTests : IDisposable
{
    private readonly string _home = Directory.CreateTempSubdirectory("bae-avalonia-smoke-").FullName;

    [Fact]
    public async Task AnEmptyHomeHasNoLibraries()
    {
        var diagnostics = NativeBae.ConfigureDiagnostics(new BridgeDiagnosticsConfig.Disabled());
        var host = BridgeHost.WithTestKeyring(diagnostics, new BridgeAppDir(_home));

        Assert.Equal(0, await NativeBae.LibraryCount(host));
    }

    public void Dispose() => Directory.Delete(_home, recursive: true);
}
