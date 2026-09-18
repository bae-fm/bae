using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class SettingsStartupTests
{
    [Fact]
    public void ConfigSnapshotDoesNotNeedALiveHandle()
    {
        var config = new BridgeConfig(
            LibraryId: "test-library", LibraryName: "Test library", LibraryPath: "/test/library",
            PauseBetweenSides: false, MaxConcurrentUploads: 2, MaxConcurrentDownloads: 2,
            IdentifyAutomatically: true, PrefillWithTags: true, LookupCatalogs: [],
            ShowRemainingTime: true, LibraryFullWidth: true, SavePresets: [],
            DefaultTrackSavePreset: "flac", DefaultReleaseSavePreset: "flac", CastEnabled: false,
            Mcp: new(true, 7890), Subsonic: new(true, 7891, "listener", "127.0.0.1"),
            DiscogsTokenStatus: BridgeDiscogsTokenStatus.NotConfigured, DiscogsUsable: false, Sync: null);

        // A delivered config is self-contained; mapping it must not ask either
        // async server for its runtime status on the dispatcher thread.
        var settings = NativeBae.SettingsFromConfig(config);

        Assert.Equal(config.LibraryName, settings.LibraryName);
        Assert.True(settings.ShowRemainingTime);
        Assert.True(settings.LibraryFullWidth);
        Assert.Equal(config.Mcp.Port, settings.McpPort);
        Assert.Equal(config.Subsonic.Username, settings.SubsonicUsername);
    }

    [Fact]
    public void FirstConfigValueInitializesTheStore()
    {
        var store = new SettingsStore(new SettingsService());
        var settings = new Settings { LibraryName = "Test library" };
        var notifications = 0;
        store.Changed += () => notifications++;

        store.ApplyConfig(settings);

        Assert.Same(settings, store.Current);
        Assert.Equal(1, notifications);
    }
}
