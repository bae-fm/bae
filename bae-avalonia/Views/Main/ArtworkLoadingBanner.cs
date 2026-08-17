using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>Post-open progress for artwork kept available on this device.</summary>
internal sealed class ArtworkLoadingBanner : Border, System.IDisposable
{
    private readonly ArtworkLoadingStore _store;
    private readonly TextBlock _title;
    private readonly TextBlock _bytes;
    private readonly TextBlock _detail;
    private readonly ProgressBar _progress;
    private readonly Button _cancel;

    public ArtworkLoadingBanner(ArtworkLoadingStore store)
    {
        _store = store;
        Padding = new Thickness(16, 8);
        BorderThickness = new Thickness(0, 0, 0, 1);
        this[!BackgroundProperty] = new DynamicResourceExtension("BaeSurfaceBrush");
        this[!BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");

        _title = new TextBlock { VerticalAlignment = VerticalAlignment.Center };
        _bytes = new TextBlock
        {
            VerticalAlignment = VerticalAlignment.Center,
            FontSize = 12,
        };
        _bytes[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");
        _cancel = new Button { Content = Loc.Chrome("action.cancel") };
        _cancel.Click += (_, _) => _store.Cancel();

        var line = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,Auto,Auto"),
            ColumnSpacing = 10,
        };
        Grid.SetColumn(_title, 0);
        Grid.SetColumn(_bytes, 1);
        Grid.SetColumn(_cancel, 2);
        line.Children.Add(_title);
        line.Children.Add(_bytes);
        line.Children.Add(_cancel);

        _progress = new ProgressBar { Height = 3, Minimum = 0, Maximum = 1 };
        _detail = new TextBlock
        {
            FontSize = 12,
            TextWrapping = TextWrapping.Wrap,
        };
        _detail[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");

        var content = new StackPanel { Spacing = 6 };
        content.Children.Add(line);
        content.Children.Add(_progress);
        content.Children.Add(_detail);
        Child = content;

        _store.Changed += Apply;
        Apply();
    }

    public void Dispose() => _store.Changed -= Apply;

    private void Apply()
    {
        _bytes.IsVisible = false;
        _detail.IsVisible = false;
        _cancel.IsVisible = false;
        _progress.IsVisible = false;
        _progress.IsIndeterminate = false;

        switch (_store.Status)
        {
            case BridgeEagerCacheFillStatus.NotRunning:
            case BridgeEagerCacheFillStatus.Complete:
                IsVisible = false;
                return;
            case BridgeEagerCacheFillStatus.Scanning scanning:
                IsVisible = true;
                _title.Text = Loc.Core(scanning.TitleKey);
                _cancel.IsVisible = true;
                _progress.IsVisible = true;
                _progress.IsIndeterminate = true;
                return;
            case BridgeEagerCacheFillStatus.Downloading downloading:
                IsVisible = true;
                _title.Text = Loc.Core(downloading.TitleKey);
                ApplyProgress(downloading.Progress);
                _cancel.IsVisible = true;
                _progress.IsVisible = true;
                _progress.Value = Fraction(downloading.Progress);
                return;
            case BridgeEagerCacheFillStatus.Cancelled cancelled:
                IsVisible = true;
                _title.Text = Loc.Core(cancelled.TitleKey);
                ApplyProgress(cancelled.Progress);
                return;
            case BridgeEagerCacheFillStatus.Failed failed:
                IsVisible = true;
                _title.Text = Loc.Core(failed.TitleKey);
                ApplyProgress(failed.Progress);
                _detail.Text = failed.Error;
                _detail.IsVisible = true;
                return;
            default:
                throw new System.InvalidOperationException(
                    $"Unknown artwork loading status {_store.Status.GetType().FullName}");
        }
    }

    private void ApplyProgress(BridgeEagerCacheFillProgress progress)
    {
        _bytes.Text = Loc.Core(
            "core.download.bytes_progress",
            new Dictionary<string, object?>
            {
                ["done"] = Loc.Bytes(checked((long)progress.BytesDone)),
                ["total"] = Loc.Bytes(checked((long)progress.BytesTotal)),
            });
        _bytes.IsVisible = true;
    }

    private static double Fraction(BridgeEagerCacheFillProgress progress)
    {
        if (progress.BytesTotal == 0)
        {
            throw new System.InvalidOperationException(
                "downloading artwork has no byte total");
        }
        return (double)progress.BytesDone / progress.BytesTotal;
    }
}
