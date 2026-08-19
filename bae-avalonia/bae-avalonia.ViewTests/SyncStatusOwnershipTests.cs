using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class SyncStatusOwnershipTests
{
    [Fact]
    public void ConfigValuesCannotOverwriteReadinessTransitions()
    {
        var settings = new SettingsStore(new SettingsService
        {
            GetSettings = () => (true, new Settings()),
        });
        settings.Reload();
        var sync = new SyncStatusStore(_ => new BridgeSyncIndicator.Idle());

        sync.Apply(Status(syncReady: true));
        settings.ApplyConfig(new Settings());
        Assert.True(sync.SyncReady);

        sync.Apply(Status(syncReady: false));
        Assert.False(sync.SyncReady);
    }

    /// A failing cycle records a whole chain; core's line names only its
    /// category. The store keeps both, or the surface has nothing to render but
    /// "Something went wrong."
    [Fact]
    public void AFailedCycleKeepsItsFaultBesideTheCategoryLine()
    {
        const string fault =
            "sync cycle: pull Store commits: database: retained Merge replay "
            + "has an unresolved foreign-key dependency";
        var sync = Store();

        sync.Apply(Failing(new BridgeException.Diagnostic(
            new BridgeErrorCategory.Internal(),
            fault)));

        Assert.Equal("category line", sync.ErrorText);
        Assert.Equal(fault, sync.ErrorDetail);
    }

    /// A recovered cycle leaves neither half behind, so the failure row goes away
    /// whole rather than showing a fault with no failure.
    [Fact]
    public void AHealthyCycleClearsBothHalves()
    {
        var sync = Store();
        sync.Apply(Failing(new BridgeException.Diagnostic(
            new BridgeErrorCategory.Network(),
            "connection reset")));

        sync.Apply(Status(syncReady: true));

        Assert.Null(sync.ErrorText);
        Assert.Null(sync.ErrorDetail);
    }

    /// A failure core keyed rather than diagnosed — a not-found — has a line and
    /// no chain. The row renders one line; it must not render a blank second one.
    [Fact]
    public void AKeyedFailureHasNoFaultToShow()
    {
        var sync = Store();

        sync.Apply(Failing(new BridgeException.NotFound(
            BridgeEntityKind.Library,
            "library-1")));

        Assert.Equal("category line", sync.ErrorText);
        Assert.Null(sync.ErrorDetail);
    }

    private static SyncStatusStore Store() =>
        new(_ => new BridgeSyncIndicator.Error(), _ => "category line");

    private static BridgeSyncStatusSnapshot Failing(BridgeException error) =>
        new(error, null, false, false);

    private static BridgeSyncStatusSnapshot Status(bool syncReady) =>
        new(null, null, false, syncReady);
}
