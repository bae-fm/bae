using System;
using System.Collections.Generic;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

internal sealed partial class ImportMappingTable
{
    private Control ArtistFillOverlay()
    {
        var canvas = new Canvas();
        var border = new Border
        {
            BorderThickness = new Thickness(2),
            IsHitTestVisible = false,
            IsVisible = false,
        };
        border[!Border.BorderBrushProperty] =
            new DynamicResourceExtension("BaeAccentBrush");
        var mark = new Border
        {
            Width = 8,
            Height = 8,
            CornerRadius = new CornerRadius(1.5),
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        mark[!Border.BackgroundProperty] =
            new DynamicResourceExtension("BaeAccentBrush");
        var handle = new Border
        {
            Width = 22,
            Height = 22,
            Background = Brushes.Transparent,
            Child = mark,
            IsVisible = false,
        };
        handle.PointerPressed += (_, e) =>
        {
            if (e.GetCurrentPoint(handle).Properties.IsLeftButtonPressed)
            {
                _draggingArtistFill = true;
                e.Pointer.Capture(handle);
                e.Handled = true;
            }
        };
        handle.PointerMoved += (_, e) =>
        {
            if (!_draggingArtistFill
                || !e.GetCurrentPoint(handle).Properties.IsLeftButtonPressed)
            {
                return;
            }
            ExtendArtistFill(e.GetPosition(canvas).Y);
            e.Handled = true;
        };
        handle.PointerReleased += (_, e) =>
        {
            if (!_draggingArtistFill || e.InitialPressMouseButton != MouseButton.Left)
            {
                return;
            }
            ExtendArtistFill(e.GetPosition(canvas).Y);
            _draggingArtistFill = false;
            e.Pointer.Capture(null);
            ApplyArtistFill();
            e.Handled = true;
        };
        handle.PointerCaptureLost += (_, _) => _draggingArtistFill = false;
        canvas.Children.Add(border);
        canvas.Children.Add(handle);
        _artistFillCanvas = canvas;
        _artistFillBorder = border;
        _artistFillHandle = handle;
        return canvas;
    }

    private void ExtendArtistFill(double y)
    {
        if (_artistFillSelection is not { } selection
            || _artistFillCanvas is not { } canvas)
        {
            return;
        }
        var candidates = Enumerable.Range(
                selection.SourceIndex,
                _artistCells.Count - selection.SourceIndex)
            .Select(index => (
                Index: index,
                Point: _artistCells[index].Cell.TranslatePoint(
                    new Point(0, 0), canvas)))
            .Where(entry => entry.Point is not null)
            .ToList();
        if (candidates.Count == 0)
        {
            return;
        }
        var target = candidates.MinBy(entry => Math.Abs(
                entry.Point!.Value.Y
                    + (_artistCells[entry.Index].Cell.Bounds.Height / 2)
                    - y));
        selection.ExtendTo(target.Index);
        UpdateArtistFillOverlay();
    }

    private void ApplyArtistFill()
    {
        if (_artistFillSelection is not { } selection)
        {
            return;
        }
        var indexes = selection.Indexes();
        if (indexes.Count < 2)
        {
            return;
        }
        _actions.SetTrackArtists(
            indexes.Select(index => _artistCells[index].TrackId).ToArray(),
            _artistCells[selection.SourceIndex].Assignments());
    }

    private void UpdateArtistFillOverlay()
    {
        if (_artistFillSelection is not { } selection
            || _artistFillCanvas is not { } canvas
            || _artistFillBorder is not { } border
            || _artistFillHandle is not { } handle)
        {
            return;
        }
        var first = _artistCells[selection.SourceIndex].Cell;
        var last = _artistCells[selection.ThroughIndex].Cell;
        var firstPoint = first.TranslatePoint(new Point(0, 0), canvas);
        var lastPoint = last.TranslatePoint(new Point(0, 0), canvas);
        if (firstPoint is null || lastPoint is null)
        {
            border.IsVisible = false;
            handle.IsVisible = false;
            return;
        }
        var left = firstPoint.Value.X;
        var top = firstPoint.Value.Y;
        var right = Math.Max(
            firstPoint.Value.X + first.Bounds.Width,
            lastPoint.Value.X + last.Bounds.Width);
        var bottom = lastPoint.Value.Y + last.Bounds.Height;
        border.Width = right - left;
        border.Height = bottom - top;
        Canvas.SetLeft(border, left);
        Canvas.SetTop(border, top);
        Canvas.SetLeft(handle, right - (handle.Width / 2));
        Canvas.SetTop(handle, bottom - (handle.Height / 2));
        border.IsVisible = true;
        handle.IsVisible = true;
    }

    private sealed record ArtistFillCell(
        string TrackId,
        Control Cell,
        Func<BridgeTrackArtistAssignments> Assignments);
}

/// <summary>The artist cell that starts a spreadsheet fill and the last cell
/// currently covered by it. Row order defines the selected range.</summary>
internal sealed class ArtistFillSelection(int sourceIndex)
{
    internal int SourceIndex { get; } = sourceIndex;
    internal int ThroughIndex { get; private set; } = sourceIndex;

    internal void ExtendTo(int index) => ThroughIndex = Math.Max(SourceIndex, index);

    internal IReadOnlyList<int> Indexes() => Enumerable
        .Range(SourceIndex, ThroughIndex - SourceIndex + 1)
        .ToArray();
}
