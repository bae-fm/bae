using System.Linq;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.LogicalTree;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class DeviceJoinProgressViewTests
{
    [AvaloniaFact]
    public void JoiningDeviceShowsTheReportedByteAndFileProgress()
    {
        var view = DeviceJoinProgressView.Build(
            new BridgeJoiningDeviceJoinProgress.DownloadingLibraryFiles(
                FilesDone: 2,
                FilesTotal: 5,
                BytesDone: 1024,
                BytesTotal: 4096));

        var progress = Assert.Single(view.GetLogicalDescendants().OfType<ProgressBar>());
        Assert.False(progress.IsIndeterminate);
        Assert.Equal(1024, progress.Value);
        Assert.Equal(4096, progress.Maximum);
        var text = view.GetLogicalDescendants().OfType<TextBlock>().Select(value => value.Text);
        Assert.Contains(Loc.Core("core.pairing.join.downloading_library_files"), text);
        Assert.Contains($"{Loc.Bytes(1024)} / {Loc.Bytes(4096)}", text);
        Assert.Contains("2 / 5", text);
    }

    [AvaloniaFact]
    public void ExistingDeviceShowsWhatItIsWaitingFor()
    {
        var view = DeviceJoinProgressView.Build(
            BridgeAdmittingDeviceJoinProgress.WaitingForJoiningDevice);

        Assert.True(Assert.Single(
            view.GetLogicalDescendants().OfType<ProgressBar>()).IsIndeterminate);
        Assert.Contains(
            Loc.Core("core.pairing.admit.waiting_for_joining_device"),
            view.GetLogicalDescendants().OfType<TextBlock>().Select(value => value.Text));
    }
}
