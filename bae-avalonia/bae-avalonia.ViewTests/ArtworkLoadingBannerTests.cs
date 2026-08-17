using System.Linq;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Interactivity;
using Avalonia.LogicalTree;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class ArtworkLoadingBannerTests
{
    [AvaloniaFact]
    public void DownloadProgressUpdatesTheMountedBannerAndCancelReachesTheOperation()
    {
        var cancelled = false;
        var store = new ArtworkLoadingStore(() => cancelled = true);
        using var banner = new ArtworkLoadingBanner(store);

        Assert.False(banner.IsVisible);

        store.Apply(new BridgeEagerCacheFillStatus.Downloading(
            TitleKey: "core.artwork_cache.downloading",
            Progress: new BridgeEagerCacheFillProgress(
                FilesDone: 1,
                FilesTotal: 3,
                BytesDone: 1024,
                BytesTotal: 4096)));

        Assert.True(banner.IsVisible);
        var progress = Assert.Single(banner.GetLogicalDescendants().OfType<ProgressBar>());
        Assert.Equal(0.25, progress.Value);
        Assert.Contains(
            Loc.Core("core.artwork_cache.downloading"),
            banner.GetLogicalDescendants().OfType<TextBlock>().Select(value => value.Text));

        var cancel = Assert.Single(banner.GetLogicalDescendants().OfType<Button>());
        cancel.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
        Assert.True(cancelled);

        store.Apply(new BridgeEagerCacheFillStatus.Complete(FilesTotal: 3, BytesTotal: 4096));
        Assert.False(banner.IsVisible);
    }
}
