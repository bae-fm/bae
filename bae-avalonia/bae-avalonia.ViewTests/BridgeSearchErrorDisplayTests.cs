using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class BridgeSearchErrorDisplayTests
{
    [Fact]
    public void ProviderFailureIncludesItsStatus()
    {
        var error = new BridgeSearchException.Lookup(
            new BridgeLookupFailure.Provider(503));

        Assert.Equal(
            "The metadata provider returned an error (503)",
            BridgeDisplay.LocalizedLine(error));
    }

    [Fact]
    public void DiagnosticKeepsItsBridgeErrorCategory()
    {
        var error = new BridgeSearchException.Diagnostic(
            new BridgeException.Diagnostic(
                new BridgeErrorCategory.Database(),
                "database unavailable"));

        Assert.Equal(
            Loc.Core("core.error.category.database"),
            BridgeDisplay.LocalizedLine(error));
    }
}
