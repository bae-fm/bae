using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The sync section's list of durable operations a completed cycle left waiting
// on a person. Each failed on a fault that running it again cannot change, so
// later cycles skip it and it moves only when someone presses Retry; a row
// leaves the list when the next sync status no longer names it. Hidden entirely
// while nothing is waiting.
//
// Three lines per row, because the kind alone identifies nothing. The kind is
// what the person reads in their own language; the description says which
// operation, and the error under it is the untranslated chain core recorded —
// the thing they can act on or paste into a bug report.
internal sealed class BlockedSyncOperationsView : StackPanel
{
    private readonly Func<string, Task<string?>> _retry;
    private readonly TextBlock _heading;
    private readonly StackPanel _rows;

    public BlockedSyncOperationsView(Func<string, Task<string?>> retry)
    {
        _retry = retry;
        Spacing = 8;
        IsVisible = false;

        _heading = new TextBlock
        {
            Text = Loc.Chrome("sync.waiting_title"),
            TextWrapping = TextWrapping.Wrap,
            FontWeight = FontWeight.SemiBold,
        };
        _heading[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");

        _rows = new StackPanel { Spacing = 12 };

        Children.Add(_heading);
        Children.Add(_rows);
    }

    /// <summary>
    /// Show the operations core is currently reporting as waiting, or hide the
    /// section when there are none.
    /// </summary>
    internal void Render(IReadOnlyList<BridgeBlockedSyncOperation> operations)
    {
        IsVisible = operations.Count > 0;
        _rows.Children.Clear();
        foreach (var operation in operations)
        {
            _rows.Children.Add(BuildRow(operation));
        }
    }

    private Control BuildRow(BridgeBlockedSyncOperation operation)
    {
        var kind = new TextBlock { Text = KindLabel(operation.Kind), TextWrapping = TextWrapping.Wrap };
        kind[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");

        var description = new TextBlock
        {
            Text = operation.Description,
            TextWrapping = TextWrapping.Wrap,
            FontSize = 12,
        };
        description[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");

        var errorText = new TextBlock
        {
            Text = operation.Error,
            TextWrapping = TextWrapping.Wrap,
            FontFamily = new FontFamily("monospace"),
            FontSize = 12,
        };
        errorText[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");

        // A retry that takes drops this row on the next sync status; one refused —
        // the operation is no longer blocked, or the loop is not running — reports
        // here rather than leaving the button looking inert.
        var status = new TextBlock { TextWrapping = TextWrapping.Wrap, IsVisible = false };
        status[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeDangerBrush");

        var button = new Button
        {
            Content = Loc.Chrome("sync.retry"),
            HorizontalAlignment = HorizontalAlignment.Left,
        };
        var id = operation.Id;
        button.Click += async (_, _) =>
        {
            status.IsVisible = false;
            button.IsEnabled = false;
            try
            {
                var failure = await _retry(id);
                if (failure is not null)
                {
                    status.Text = failure;
                    status.IsVisible = true;
                }
            }
            finally
            {
                button.IsEnabled = true;
            }
        };

        return new StackPanel
        {
            Spacing = 4,
            Children = { kind, description, errorText, status, button },
        };
    }

    private static string KindLabel(BridgeBlockedSyncOperationKind kind) =>
        kind switch
        {
            BridgeBlockedSyncOperationKind.Write => Loc.Chrome("sync.blocked.write"),
            BridgeBlockedSyncOperationKind.CircleOperation =>
                Loc.Chrome("sync.blocked.circle_operation"),
            BridgeBlockedSyncOperationKind.Reclaim => Loc.Chrome("sync.blocked.reclaim"),
            _ => throw new ArgumentOutOfRangeException(
                nameof(kind), kind, "Unknown blocked sync operation kind"),
        };
}
