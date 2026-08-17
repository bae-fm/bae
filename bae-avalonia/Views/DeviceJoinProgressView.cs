using Avalonia.Controls;
using Avalonia.Layout;
using uniffi.bae_bridge;

namespace Bae.Desktop;

internal static class DeviceJoinProgressView
{
    internal static Control Build(BridgeJoiningDeviceJoinProgress progress)
    {
        var column = new StackPanel
        {
            Spacing = 6,
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        var bytes = progress switch
        {
            BridgeJoiningDeviceJoinProgress.DownloadingSnapshot value =>
                (value.BytesDone, value.BytesTotal),
            BridgeJoiningDeviceJoinProgress.DownloadingLibraryFiles value =>
                (value.BytesDone, value.BytesTotal),
            _ => ((ulong Done, ulong Total)?)null,
        };
        column.Children.Add(bytes is { Total: > 0 }
            ? new ProgressBar
            {
                Minimum = 0,
                Maximum = checked((double)bytes.Value.Total),
                Value = checked((double)bytes.Value.Done),
            }
            : new ProgressBar { IsIndeterminate = true });
        column.Children.Add(DialogUi.Body(Loc.Core(
            BaeBridgeMethods.BridgeJoiningDeviceJoinProgressKey(progress))));
        if (bytes is { } transfer)
        {
            column.Children.Add(DialogUi.Body(
                $"{Loc.Bytes(checked((long)transfer.Done))} / {Loc.Bytes(checked((long)transfer.Total))}"));
        }
        if (progress is BridgeJoiningDeviceJoinProgress.DownloadingLibraryFiles files)
        {
            column.Children.Add(DialogUi.Body(
                $"{files.FilesDone.ToString("N0")} / {files.FilesTotal.ToString("N0")}"));
        }
        return column;
    }

    internal static Control Build(BridgeAdmittingDeviceJoinProgress progress)
    {
        var column = new StackPanel { Spacing = 6 };
        column.Children.Add(new ProgressBar { IsIndeterminate = true });
        column.Children.Add(DialogUi.Body(Loc.Core(
            BaeBridgeMethods.BridgeAdmittingDeviceJoinProgressKey(progress))));
        return column;
    }
}
