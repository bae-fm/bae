using System;
using System.Globalization;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// A running import's step, how far through it is, and the bar.
///
/// One component for both the surfaces that show it — the candidate's row in
/// the list, and its card in the mapping pane — because they are answering the
/// same question about the same import, and two renderings of one run is two
/// chances to disagree about it. Each keeps itself current from the
/// candidate-runtime signal, filtered to its own key, so a tick redraws this
/// and nothing around it.
/// </summary>
internal static class ImportProgressLine
{
    internal static Control Build(ImportStore import, string candidateKey)
    {
        var text = new TextBlock
        {
            FontSize = 12.5,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
            Margin = new Thickness(0, 1, 0, 0),
        };
        text[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var barHost = new Border { Margin = new Thickness(0, 7, 0, 0) };
        var column = new StackPanel { Spacing = 0, Children = { text, barHost } };
        CandidateRuntimeObserver.Attach(column, import, candidateKey, runtime =>
        {
            // A candidate placed as importing whose run has not reported yet is
            // at the start with no step named.
            var percent = runtime?.Import?.ProgressPercent;
            var step = runtime?.Import?.Step;
            var phase = step is { } named
                ? BridgeDisplay.LocalizedLine(named)
                : Loc.Chrome("import.progress.identifying");
            text.Text = percent is { } value
                ? $"{phase} · {(value / 100.0).ToString("P0", CultureInfo.CurrentCulture)}"
                : phase;
            barHost.Child = Bar(percent is { } progress ? progress / 100.0 : null);
        });
        return column;
    }

    /// <summary>The bar itself: a filled run over a track, three points tall.</summary>
    internal static ProgressBar Bar(double? fraction)
    {
        return new ProgressBar
        {
            Height = 3,
            Minimum = 0,
            Maximum = 1,
            Value = Math.Clamp(fraction ?? 0, 0, 1),
            IsIndeterminate = fraction is null,
        };
    }
}
