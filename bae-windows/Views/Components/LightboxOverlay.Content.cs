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

// The main image's state machine: rendering the current entry, loading its
// bytes, and resetting the zoom/scroll between entries. Split out of
// LightboxOverlay.cs unchanged.
internal sealed partial class LightboxOverlay
{
    // Render the current state: label, counter, chrome visibility, navigation
    // buttons, thumbnails.
    private void RenderState()
    {
        if (_state is null || _labelText is null || _counterText is null)
        {
            return;
        }

        var entry = _entries[_state.Index];
        _labelText.Text = entry.Label;

        var (counterKey, counterArgs) = LightboxModel.Counter(_state);
        _counterText.Text = Loc.Chrome(counterKey, counterArgs);

        var canCycle = LightboxModel.CanCycle(_state);
        if (_prevButton is not null)
        {
            _prevButton.Visibility = canCycle ? Visibility.Visible : Visibility.Collapsed;
        }

        if (_nextButton is not null)
        {
            _nextButton.Visibility = canCycle ? Visibility.Visible : Visibility.Collapsed;
        }

        if (_thumbnailPanel is not null && _thumbnailScroll is not null)
        {
            _thumbnailScroll.Visibility = canCycle ? Visibility.Visible : Visibility.Collapsed;

            // Populate the thumbnail panel with tiles.
            _thumbnailPanel.Children.Clear();
            for (int i = 0; i < _state.Count; i++)
            {
                var entry_i = _entries[i];
                BitmapImage? thumbnail = null;

                if (_thumbnailCache.TryGetValue(entry_i.Id, out var cached))
                {
                    thumbnail = cached;
                }

                var item = new ThumbnailItem(entry_i.Id, entry_i.Label, i, i == _state.Index, thumbnail);
                var tile = BuildThumbnailTile(item);
                _thumbnailPanel.Children.Add(tile);
            }
        }
    }

    // Load the current entry's image bytes asynchronously off the UI thread.
    // Stale guard by entry id: if the entry changes while loading, discard the result.
    private void LoadEntry()
    {
        if (_state is null || _image is null || _progressRing is null || _failureView is null)
        {
            return;
        }

        var entry = _entries[_state.Index];
        _loadingEntryId = entry.Id;

        // Reset UI state.
        _image.Source = null;
        _failureView.Visibility = Visibility.Collapsed;
        _progressRing.IsActive = true;

        var loadingId = entry.Id;
        var readBytes = entry.ReadBytes;

        _ = Task.Run(readBytes).ContinueWith(task =>
        {
            byte[]? bytes = null;
            if (task.Status == System.Threading.Tasks.TaskStatus.RanToCompletion)
            {
                bytes = task.Result;
            }
            else if (task.Exception is not null)
            {
                BaeDiagnostics.Logger.Warning("Failed to read lightbox image.", task.Exception);
            }

            // Dispatch back to UI thread. Stale guard: if the entry changed
            // while we were loading, discard this result.
            var xamlRoot = _xamlRoot?.Invoke();
            if (xamlRoot is not null)
            {
                _popup.DispatcherQueue.TryEnqueue(() =>
                {
                    if (_loadingEntryId != loadingId)
                    {
                        return;
                    }

                    _progressRing.IsActive = false;

                    if (bytes is null)
                    {
                        _failureView.Visibility = Visibility.Visible;
                        return;
                    }

                    try
                    {
                        using var stream = new System.IO.MemoryStream(bytes);
                        var bitmap = new BitmapImage();
                        bitmap.SetSource(stream.AsRandomAccessStream());
                        _image.Source = bitmap;
                    }
                    catch (Exception)
                    {
                        _failureView.Visibility = Visibility.Visible;
                    }
                });
            }
        }, System.Threading.Tasks.TaskScheduler.Default);
    }

    // Reset the ScrollViewer to zoom 1.0 and scroll position (0, 0).
    private void ResetScrollViewer()
    {
        if (_scrollViewer is null)
        {
            return;
        }

        _scrollViewer.ChangeView(0, 0, (float)LightboxModel.MinZoom, disableAnimation: false);
    }
}
