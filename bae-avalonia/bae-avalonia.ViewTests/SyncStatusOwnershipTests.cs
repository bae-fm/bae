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

    private static BridgeSyncStatusSnapshot Status(bool syncReady) =>
        new(null, null, false, syncReady);
}
