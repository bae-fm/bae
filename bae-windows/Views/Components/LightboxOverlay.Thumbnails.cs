using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;

namespace Bae.Windows;

// The thumbnail strip: loading the tiles, the tile model, tile construction,
// and selection. Split out of LightboxOverlay.cs unchanged.
internal sealed partial class LightboxOverlay
{
    // Load all thumbnails asynchronously. Thumbnails are cached in
    // _thumbnailCache by entry id, so subsequent shows reuse them. Each thumbnail
    // is decoded at 56 px (the tile width/height).
    private void LoadThumbnails()
    {
        if (_entries.Count == 0)
        {
            return;
        }

        var xamlRoot = _xamlRoot?.Invoke();
        if (xamlRoot is null)
        {
            return;
        }

        // Start loading thumbnails for any uncached entries.
        foreach (var entry in _entries)
        {
            if (_thumbnailCache.ContainsKey(entry.Id))
            {
                continue;
            }

            var readBytes = entry.ReadBytes;
            var entryId = entry.Id;

            _ = Task.Run(readBytes).ContinueWith(task =>
            {
                byte[]? bytes = null;
                if (task.Status == System.Threading.Tasks.TaskStatus.RanToCompletion)
                {
                    bytes = task.Result;
                }

                _popup.DispatcherQueue.TryEnqueue(() =>
                {
                    BitmapImage? thumbnail = null;
                    if (bytes is not null)
                    {
                        try
                        {
                            using var stream = new System.IO.MemoryStream(bytes);
                            thumbnail = new BitmapImage();
                            thumbnail.SetSource(stream.AsRandomAccessStream());
                        }
                        catch (Exception)
                        {
                            // Undecodable bytes; leave thumbnail null.
                        }
                    }

                    _thumbnailCache[entryId] = thumbnail;

                    // Re-render to show the now-loaded thumbnail.
                    if (_state is not null)
                    {
                        RenderState();
                    }
                });
            }, System.Threading.Tasks.TaskScheduler.Default);
        }
    }

    // Thumbnail item in the strip: entry id, label, index, whether it's active,
    // and its cached decoded BitmapImage (at 56 px).
    private sealed record ThumbnailItem(
        string Id,
        string Label,
        int Index,
        bool IsActive,
        BitmapImage? Thumbnail);

    // Build a thumbnail tile for the repeater. Called by the ItemsRepeater's
    // DataTemplate; the item is bound when rendering.
    private Grid BuildThumbnailTile(ThumbnailItem item)
    {
        var thumbnail = item.Thumbnail;
        var isActive = item.IsActive;
        var index = item.Index;

        var borderBrush = isActive
            ? new SolidColorBrush(Microsoft.UI.Colors.DeepSkyBlue)
            : new SolidColorBrush(Microsoft.UI.Colors.Transparent);

        var tile = new Grid
        {
            Width = 56,
            Height = 56,
            BorderThickness = isActive ? new Thickness(2) : new Thickness(0),
            BorderBrush = borderBrush,
        };

        if (thumbnail is not null)
        {
            var image = new Image
            {
                Source = thumbnail,
                Stretch = Stretch.UniformToFill,
            };
            tile.Children.Add(image);
        }

        var button = new Button
        {
            Content = tile,
            Width = 56,
            Height = 56,
            Padding = new Thickness(0),
        };

        button.Click += (_, _) => SelectThumbnail(index);

        var container = new Grid();
        container.Children.Add(button);
        return container;
    }

    private void SelectThumbnail(int index)
    {
        if (_state is null)
        {
            return;
        }

        var newState = LightboxModel.Select(_state, index);
        if (newState.Index != _state.Index)
        {
            _state = newState;
            RenderState();
            ResetScrollViewer();
            LoadEntry();
        }
    }
}
