using System;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;

namespace Bae.Desktop;

// The sync section's failure row: what core reports when a cycle fails, plus the
// retry for the provider this library is already configured for.
//
// Two lines, because one is not enough. Core's line names a category
// ("Something went wrong."), which identifies nothing on its own; the fault line
// under it is the untranslated chain core recorded — the thing a user can act on
// or paste into a bug report. Hidden entirely while sync is healthy.
internal sealed class SyncFailureView : StackPanel
{
    internal TextBlock LineText { get; }
    internal TextBlock DetailText { get; }
    internal Button ReconnectButton { get; }

    public SyncFailureView(Func<Task> reconnect)
    {
        Spacing = 4;
        IsVisible = false;

        LineText = new TextBlock { TextWrapping = TextWrapping.Wrap };
        LineText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeDangerBrush");

        DetailText = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            FontFamily = new FontFamily("monospace"),
            FontSize = 12,
            IsVisible = false,
        };
        DetailText[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");

        // The retry runs over the network, so the button stays busy until it
        // settles. It settles visibly either way: a retry that took clears the
        // error and takes this row with it, and one that didn't leaves the row
        // showing whatever core recorded instead.
        ReconnectButton = new Button
        {
            Content = Loc.Chrome("sync.reconnect"),
            HorizontalAlignment = HorizontalAlignment.Left,
        };
        ReconnectButton.Click += async (_, _) =>
        {
            ReconnectButton.IsEnabled = false;
            try
            {
                await reconnect();
            }
            finally
            {
                ReconnectButton.IsEnabled = true;
            }
        };

        Children.Add(LineText);
        Children.Add(DetailText);
        Children.Add(ReconnectButton);
    }

    /// <summary>
    /// Show the failure core is currently reporting, or hide the row when there
    /// is none. `line` absent means sync is healthy; `detail` absent means the
    /// failure carries no diagnostic to name, which is a row with one line, never
    /// a blank second one.
    /// </summary>
    internal void Render(string? line, string? detail, bool canReconnect)
    {
        IsVisible = line is not null;
        ReconnectButton.IsVisible = canReconnect;
        LineText.Text = line ?? string.Empty;
        DetailText.Text = detail ?? string.Empty;
        DetailText.IsVisible = detail is not null;
    }
}
