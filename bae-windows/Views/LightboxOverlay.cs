using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;

namespace Bae.Windows;

// A single entry in the lightbox: its id, display label, and a closure that
// reads the image bytes on demand. The closure returns null on I/O failure or
// when the library handle is no longer current (e.g., during a library switch).
internal sealed record LightboxEntry(string Id, string Label, Func<byte[]?> ReadBytes);

// The lightbox overlay: a Popup-hosted full-window scrim with zoom/pan, keyboard
// navigation, and a thumbnail strip. Opens from the album detail gallery button
// and the import confirmation's local artwork tiles.
internal sealed class LightboxOverlay
{
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Popup _popup = new();
    private LightboxState? _state;
    private IReadOnlyList<LightboxEntry> _entries = Array.Empty<LightboxEntry>();
    private string? _loadingEntryId;
    private ScrollViewer? _scrollViewer;
    private Image? _image;
    private ProgressRing? _progressRing;
    private Grid? _failureView;
    private Grid? _chrome;
    private Button? _closeButton;
    private Button? _prevButton;
    private Button? _nextButton;
    private TextBlock? _labelText;
    private TextBlock? _counterText;
    private StackPanel? _thumbnailPanel;
    private ScrollViewer? _thumbnailScroll;
    private Grid? _root;

    // Thumbnail cache: entry id → decoded BitmapImage at thumbnail size (56 px).
    // Persists across Show calls so a subsequent lightbox open reuses thumbnails.
    private readonly Dictionary<string, BitmapImage?> _thumbnailCache = new();

    public LightboxOverlay(Func<XamlRoot?> xamlRoot)
    {
        _xamlRoot = xamlRoot;
    }

    public bool IsOpen => _popup.IsOpen;

    // Open the lightbox over the given entries, starting at startIndex. If the
    // entries list is empty or null, Hide() is called instead.
    public void Show(IReadOnlyList<LightboxEntry> entries, int startIndex)
    {
        if (entries is null || entries.Count == 0)
        {
            Hide();
            return;
        }

        _entries = entries;
        var state = LightboxModel.Open(entries.Count, startIndex);
        if (state is null)
        {
            Hide();
            return;
        }

        _state = state;
        BuildUI();
        RenderState();
        _root?.Focus(FocusState.Programmatic);
        _popup.IsOpen = true;
        LoadThumbnails();
        LoadEntry();
    }

    public void Hide()
    {
        _popup.IsOpen = false;
        _state = null;
        _loadingEntryId = null;
    }

    // Build the overlay's visual tree once on first Show.
    private void BuildUI()
    {
        if (_root is not null)
        {
            return;
        }

        // Root grid: full-window, receives focus and keyboard.
        _root = new Grid
        {
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            IsTabStop = true,
        };
        _root.KeyDown += OnKeyDown;

        // Scrim: black background at 0.85 opacity; Tapped on the scrim (not the
        // image) dismisses the lightbox.
        var scrim = new Grid
        {
            Background = new SolidColorBrush(Microsoft.UI.Colors.Black),
            Opacity = 0.85,
        };
        scrim.Tapped += (_, _) => Hide();
        _root.Children.Add(scrim);

        // Image display: centered, fit-to-window at zoom 1.0, with zoom/pan via
        // ScrollViewer. MinZoomFactor and MaxZoomFactor are set from the model.
        _scrollViewer = new ScrollViewer
        {
            ZoomMode = ZoomMode.Enabled,
            MinZoomFactor = (float)LightboxModel.MinZoom,
            MaxZoomFactor = (float)LightboxModel.MaxZoom,
            HorizontalScrollMode = ScrollMode.Enabled,
            VerticalScrollMode = ScrollMode.Enabled,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Hidden,
            VerticalScrollBarVisibility = ScrollBarVisibility.Hidden,
        };
        _scrollViewer.ViewChanged += OnScrollViewerViewChanged;

        _image = new Image
        {
            Stretch = Stretch.Uniform,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        _image.DoubleTapped += OnImageDoubleTapped;

        _scrollViewer.Content = _image;
        _root.Children.Add(_scrollViewer);

        // Chrome: close button, nav chevrons, label, counter, thumbnail strip.
        // All fade to opacity 0 when zoomed.
        _chrome = BuildChrome();
        _root.Children.Add(_chrome);

        // Failure view: shown when image bytes are null or undecodable.
        _failureView = BuildFailureView();
        _root.Children.Add(_failureView);

        // Progress ring: shown while loading.
        _progressRing = new ProgressRing
        {
            IsActive = false,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        _root.Children.Add(_progressRing);

        _popup.Child = _root;
        var xamlRoot = _xamlRoot?.Invoke();
        if (xamlRoot is not null)
        {
            _popup.XamlRoot = xamlRoot;
        }

        // Resize the popup when the window changes.
        if (xamlRoot is not null)
        {
            xamlRoot.Changed += OnXamlRootChanged;
            ResizePopup(xamlRoot.Size);
        }
    }

    private Grid BuildChrome()
    {
        var chrome = new Grid
        {
            IsTabStop = false,
        };

        // Top-right close button: circular, always visible.
        _closeButton = new Button
        {
            Content = new SymbolIcon { Symbol = Symbol.Cancel },
            Width = 40,
            Height = 40,
            CornerRadius = new CornerRadius(20),
            HorizontalAlignment = HorizontalAlignment.Right,
            VerticalAlignment = VerticalAlignment.Top,
            Margin = new Thickness(0, 16, 16, 0),
        };
        ToolTipService.SetToolTip(_closeButton, Loc.Chrome("action.close"));
        _closeButton.Click += (_, _) => Hide();
        chrome.Children.Add(_closeButton);

        // Navigation and label container: centered layout below the image.
        var infoContainer = new StackPanel
        {
            Orientation = Orientation.Vertical,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Bottom,
            Spacing = 12,
            Margin = new Thickness(0, 0, 0, 16),
        };

        // Label + counter on a single line.
        var labelLine = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            HorizontalAlignment = HorizontalAlignment.Center,
            Spacing = 8,
        };

        _labelText = new TextBlock
        {
            TextAlignment = TextAlignment.Center,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.White),
        };
        labelLine.Children.Add(_labelText);

        _counterText = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.White),
        };
        labelLine.Children.Add(_counterText);

        infoContainer.Children.Add(labelLine);

        // Navigation buttons and thumbnail strip.
        var navContainer = new StackPanel
        {
            Orientation = Orientation.Vertical,
            HorizontalAlignment = HorizontalAlignment.Center,
            Spacing = 12,
        };

        // Chevron buttons (shown only when CanCycle).
        var chevronRow = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            HorizontalAlignment = HorizontalAlignment.Center,
            Spacing = 24,
        };

        _prevButton = new Button
        {
            Content = new TextBlock { Text = "‹", FontSize = 20 },
            Width = 40,
            Height = 40,
            CornerRadius = new CornerRadius(20),
            Padding = new Thickness(0),
        };
        ToolTipService.SetToolTip(_prevButton, Loc.Chrome("gallery.previous"));
        AutomationProperties.SetName(_prevButton, Loc.Chrome("gallery.previous"));
        _prevButton.Click += (_, _) => OnPrevious();
        chevronRow.Children.Add(_prevButton);

        _nextButton = new Button
        {
            Content = new TextBlock { Text = "›", FontSize = 20 },
            Width = 40,
            Height = 40,
            CornerRadius = new CornerRadius(20),
            Padding = new Thickness(0),
        };
        ToolTipService.SetToolTip(_nextButton, Loc.Chrome("gallery.next"));
        AutomationProperties.SetName(_nextButton, Loc.Chrome("gallery.next"));
        _nextButton.Click += (_, _) => OnNext();
        chevronRow.Children.Add(_nextButton);

        navContainer.Children.Add(chevronRow);

        // Thumbnail strip: 56 px tiles with active highlight, click to select.
        // Only shown when CanCycle.
        _thumbnailPanel = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 4,
        };

        _thumbnailScroll = new ScrollViewer
        {
            Content = _thumbnailPanel,
            HorizontalScrollMode = ScrollMode.Auto,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Auto,
            VerticalScrollBarVisibility = ScrollBarVisibility.Hidden,
            MaxWidth = 600,
        };

        navContainer.Children.Add(_thumbnailScroll);

        infoContainer.Children.Add(navContainer);
        chrome.Children.Add(infoContainer);

        return chrome;
    }

    private Grid BuildFailureView()
    {
        var failureView = new Grid
        {
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
            Visibility = Visibility.Collapsed,
        };

        var stack = new StackPanel
        {
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
            Spacing = 12,
        };

        var icon = new SymbolIcon
        {
            Symbol = Symbol.Warning,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.White),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        stack.Children.Add(icon);

        var message = new TextBlock
        {
            Text = Loc.Chrome("gallery.image_load_failed"),
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.White),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        stack.Children.Add(message);

        failureView.Children.Add(stack);

        return failureView;
    }

    // Resize the popup to match the window size. global:: because inside
    // namespace Bae.Windows the bare `Windows` resolves to Bae.Windows, not
    // the platform namespace (CS0234).
    private void ResizePopup(global::Windows.Foundation.Size windowSize)
    {
        _popup.Width = windowSize.Width;
        _popup.Height = windowSize.Height;
    }

    private void OnXamlRootChanged(XamlRoot sender, XamlRootChangedEventArgs args)
    {
        ResizePopup(sender.Size);
    }

    // Keyboard navigation: Escape closes, Left/Right navigate.
    private void OnKeyDown(object sender, KeyRoutedEventArgs e)
    {
        switch (e.Key)
        {
            case Windows.System.VirtualKey.Escape:
                Hide();
                e.Handled = true;
                break;
            case Windows.System.VirtualKey.Left:
                OnPrevious();
                e.Handled = true;
                break;
            case Windows.System.VirtualKey.Right:
                OnNext();
                e.Handled = true;
                break;
        }
    }

    // Double-tap: zoom to DoubleTapTarget centered on the tap point.
    private void OnImageDoubleTapped(object sender, DoubleTappedRoutedEventArgs e)
    {
        if (_state is null || _scrollViewer is null)
        {
            return;
        }

        var targetZoom = (float)LightboxModel.DoubleTapTarget(_scrollViewer.ZoomFactor);
        var tapPoint = e.GetPosition(_image);

        // Calculate the viewport center point that should align with the tap point.
        var viewportWidth = _scrollViewer.ViewportWidth;
        var viewportHeight = _scrollViewer.ViewportHeight;
        var horizontalOffset = (float)(tapPoint.X * targetZoom - viewportWidth / 2);
        var verticalOffset = (float)(tapPoint.Y * targetZoom - viewportHeight / 2);

        _scrollViewer.ChangeView(horizontalOffset, verticalOffset, targetZoom, disableAnimation: false);
    }

    // ScrollViewer.ViewChanged is called when zoom factor changes (either by
    // pinch, scroll wheel, or ChangeView). Update chrome visibility and chrome
    // fading based on the current zoom.
    private void OnScrollViewerViewChanged(object sender, ScrollViewerViewChangedEventArgs e)
    {
        if (_scrollViewer is null || _state is null || _chrome is null)
        {
            return;
        }

        var zoom = _scrollViewer.ZoomFactor;
        var chromeVisible = LightboxModel.ChromeVisible(zoom);
        _chrome.Opacity = chromeVisible ? 1.0 : 0.0;
        _chrome.IsHitTestVisible = chromeVisible;
    }

    // Navigation actions: update state, reset scroll viewer, load new entry.
    private void OnNext()
    {
        if (_state is null)
        {
            return;
        }

        _state = LightboxModel.Next(_state);
        RenderState();
        ResetScrollViewer();
        LoadEntry();
    }

    private void OnPrevious()
    {
        if (_state is null)
        {
            return;
        }

        _state = LightboxModel.Previous(_state);
        RenderState();
        ResetScrollViewer();
        LoadEntry();
    }

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

                xamlRoot.Dispatcher.TryEnqueue(() =>
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
                xamlRoot.Dispatcher.TryEnqueue(() =>
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
