using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using Avalonia.Media.Imaging;
using Avalonia.Threading;

namespace Bae.Desktop;

// One entry in the lightbox: its id, display label, and a closure that reads the
// image bytes on demand (null on I/O failure or a stale library handle).
internal sealed record LightboxEntry(string Id, string Label, Func<byte[]?> ReadBytes);

// The gallery lightbox: a full-window overlay (in the window root, beside the
// modal host) with a fit-to-window image, wheel / double-tap zoom and drag pan,
// keyboard navigation, a label + counter, and a thumbnail strip. The navigation and
// zoom decision logic live in the pure, tested LightboxModel; this renders it.
// Avalonia has no ScrollViewer zoom, so zoom is a render transform on the image.
internal sealed class LightboxOverlay : Panel
{
    private readonly Border _scrim;
    private readonly Image _image;
    private readonly ScaleTransform _scale = new(1, 1);
    private readonly TranslateTransform _translate = new(0, 0);
    private readonly Spinner _spinner;
    private readonly Control _failure;
    private readonly Grid _chrome;
    private readonly Button _prev;
    private readonly Button _next;
    private readonly TextBlock _label;
    private readonly TextBlock _counter;
    private readonly StackPanel _thumbnails;
    private readonly ScrollViewer _thumbnailScroll;

    private readonly Dictionary<string, Bitmap?> _thumbnailCache = new();
    private IReadOnlyList<LightboxEntry> _entries = Array.Empty<LightboxEntry>();
    private LightboxState? _state;
    private string? _loadingEntryId;
    private Point? _panOrigin;

    public LightboxOverlay()
    {
        IsVisible = false;
        Focusable = true;

        _scrim = new Border { Background = new SolidColorBrush(Colors.Black, 0.85) };
        _scrim.PointerPressed += (_, _) => Hide();
        Children.Add(_scrim);

        _image = new Image
        {
            Stretch = Stretch.Uniform,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
            RenderTransformOrigin = RelativePoint.Center,
            RenderTransform = new TransformGroup { Children = { _scale, _translate } },
        };
        var imageHost = new Panel { Margin = new Thickness(64) };
        imageHost.Children.Add(_image);
        imageHost.PointerWheelChanged += OnWheel;
        imageHost.DoubleTapped += OnDoubleTapped;
        imageHost.PointerPressed += OnPointerPressed;
        imageHost.PointerMoved += OnPointerMoved;
        imageHost.PointerReleased += (_, _) => _panOrigin = null;
        Children.Add(imageHost);

        _spinner = new Spinner
        {
            Width = 32,
            Height = 32,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
            IsVisible = false,
        };
        Children.Add(_spinner);

        _failure = BuildFailure();
        Children.Add(_failure);

        _prev = NavButton("‹", Loc.Chrome("gallery.previous"), OnPrevious, HorizontalAlignment.Left);
        _next = NavButton("›", Loc.Chrome("gallery.next"), OnNext, HorizontalAlignment.Right);
        _label = ChromeText(15);
        _counter = ChromeText(13);
        _thumbnails = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        _thumbnailScroll = new ScrollViewer
        {
            Content = _thumbnails,
            HorizontalScrollBarVisibility = Avalonia.Controls.Primitives.ScrollBarVisibility.Auto,
            VerticalScrollBarVisibility = Avalonia.Controls.Primitives.ScrollBarVisibility.Disabled,
            MaxWidth = 620,
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        _chrome = BuildChrome();
        Children.Add(_chrome);

        KeyDown += OnKeyDown;
    }

    // Open the lightbox over the given entries, starting at startIndex.
    public void Show(IReadOnlyList<LightboxEntry> entries, int startIndex)
    {
        var state = LightboxModel.Open(entries.Count, startIndex);
        if (state is null)
        {
            Hide();
            return;
        }
        _entries = entries;
        _state = state;
        IsVisible = true;
        Focus();
        RenderState();
        LoadThumbnails();
        LoadEntry();
    }

    public void Hide()
    {
        IsVisible = false;
        _state = null;
        _loadingEntryId = null;
    }

    // ── Render ────────────────────────────────────────────────────────────────
    private void RenderState()
    {
        if (_state is not { } state)
        {
            return;
        }
        _label.Text = _entries[state.Index].Label;
        var (key, args) = LightboxModel.Counter(state);
        _counter.Text = Loc.Chrome(key, args);

        var canCycle = LightboxModel.CanCycle(state);
        _prev.IsVisible = canCycle;
        _next.IsVisible = canCycle;
        _thumbnailScroll.IsVisible = canCycle;

        _thumbnails.Children.Clear();
        for (var i = 0; i < state.Count; i++)
        {
            _thumbnailCache.TryGetValue(_entries[i].Id, out var thumb);
            _thumbnails.Children.Add(BuildThumbnailTile(i, thumb, i == state.Index));
        }
        ApplyZoom(state.Zoom);
    }

    private void LoadEntry()
    {
        if (_state is not { } state)
        {
            return;
        }
        var entry = _entries[state.Index];
        _loadingEntryId = entry.Id;
        _image.Source = null;
        _failure.IsVisible = false;
        _spinner.IsVisible = true;
        var loadingId = entry.Id;
        var read = entry.ReadBytes;
        _ = Task.Run(read).ContinueWith(
            task =>
            {
                var bytes = task.Status == TaskStatus.RanToCompletion ? task.Result : null;
                if (_loadingEntryId != loadingId)
                {
                    return;
                }
                _spinner.IsVisible = false;
                var bitmap = Decode(bytes);
                if (bitmap is null)
                {
                    _failure.IsVisible = true;
                    return;
                }
                _image.Source = bitmap;
            },
            TaskScheduler.FromCurrentSynchronizationContext());
    }

    private void LoadThumbnails()
    {
        foreach (var entry in _entries)
        {
            if (_thumbnailCache.ContainsKey(entry.Id))
            {
                continue;
            }
            var id = entry.Id;
            var read = entry.ReadBytes;
            _ = Task.Run(read).ContinueWith(
                task =>
                {
                    var bytes = task.Status == TaskStatus.RanToCompletion ? task.Result : null;
                    _thumbnailCache[id] = Decode(bytes);
                    if (_state is not null)
                    {
                        RenderState();
                    }
                },
                TaskScheduler.FromCurrentSynchronizationContext());
        }
    }

    // ── Navigation ────────────────────────────────────────────────────────────
    private void OnNext()
    {
        if (_state is { } state)
        {
            _state = LightboxModel.Next(state);
            RenderState();
            LoadEntry();
        }
    }

    private void OnPrevious()
    {
        if (_state is { } state)
        {
            _state = LightboxModel.Previous(state);
            RenderState();
            LoadEntry();
        }
    }

    private void SelectThumbnail(int index)
    {
        if (_state is { } state && LightboxModel.Select(state, index) is var next && next.Index != state.Index)
        {
            _state = next;
            RenderState();
            LoadEntry();
        }
    }

    private void OnKeyDown(object? sender, KeyEventArgs e)
    {
        switch (e.Key)
        {
            case Key.Escape:
                Hide();
                e.Handled = true;
                break;
            case Key.Left:
                OnPrevious();
                e.Handled = true;
                break;
            case Key.Right:
                OnNext();
                e.Handled = true;
                break;
        }
    }

    // ── Zoom / pan ────────────────────────────────────────────────────────────
    private void OnWheel(object? sender, PointerWheelEventArgs e)
    {
        if (_state is not { } state)
        {
            return;
        }
        var zoom = LightboxModel.ClampZoom(state.Zoom * (1 + e.Delta.Y * 0.1));
        _state = state with { Zoom = zoom };
        ApplyZoom(zoom);
        e.Handled = true;
    }

    private void OnDoubleTapped(object? sender, TappedEventArgs e)
    {
        if (_state is not { } state)
        {
            return;
        }
        var zoom = LightboxModel.DoubleTapTarget(state.Zoom);
        _state = state with { Zoom = zoom };
        ApplyZoom(zoom);
        e.Handled = true;
    }

    private void OnPointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (_state is { Zoom: > 1.01 } && e.GetCurrentPoint(this).Properties.IsLeftButtonPressed)
        {
            _panOrigin = e.GetPosition(this);
        }
    }

    private void OnPointerMoved(object? sender, PointerEventArgs e)
    {
        if (_panOrigin is not { } origin)
        {
            return;
        }
        var point = e.GetPosition(this);
        _translate.X += point.X - origin.X;
        _translate.Y += point.Y - origin.Y;
        _panOrigin = point;
    }

    private void ApplyZoom(double zoom)
    {
        _scale.ScaleX = zoom;
        _scale.ScaleY = zoom;
        if (zoom <= 1.01)
        {
            _translate.X = 0;
            _translate.Y = 0;
        }
        // Chrome fades out while zoomed so it doesn't sit over the image.
        var visible = LightboxModel.ChromeVisible(zoom);
        _chrome.Opacity = visible ? 1 : 0;
        _chrome.IsHitTestVisible = visible;
    }

    // ── Building blocks ───────────────────────────────────────────────────────
    private Grid BuildChrome()
    {
        var chrome = new Grid();
        var close = NavButton("✕", Loc.Chrome("action.close"), Hide, HorizontalAlignment.Right);
        close.VerticalAlignment = VerticalAlignment.Top;
        close.Margin = new Thickness(0, 16, 16, 0);
        chrome.Children.Add(close);

        _prev.VerticalAlignment = VerticalAlignment.Center;
        _prev.Margin = new Thickness(16, 0, 0, 0);
        _next.VerticalAlignment = VerticalAlignment.Center;
        _next.Margin = new Thickness(0, 0, 16, 0);
        chrome.Children.Add(_prev);
        chrome.Children.Add(_next);

        var info = new StackPanel
        {
            Orientation = Orientation.Vertical,
            Spacing = 12,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Bottom,
            Margin = new Thickness(0, 0, 0, 20),
        };
        var labelLine = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, HorizontalAlignment = HorizontalAlignment.Center };
        labelLine.Children.Add(_label);
        labelLine.Children.Add(_counter);
        info.Children.Add(labelLine);
        info.Children.Add(_thumbnailScroll);
        chrome.Children.Add(info);
        return chrome;
    }

    private Control BuildFailure()
    {
        var text = new TextBlock
        {
            Text = Loc.Chrome("gallery.image_load_failed"),
            Foreground = Brushes.White,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        text.IsVisible = false;
        return text;
    }

    private Control BuildThumbnailTile(int index, Bitmap? thumbnail, bool active)
    {
        var image = new Image { Stretch = Stretch.UniformToFill };
        if (thumbnail is not null)
        {
            image.Source = thumbnail;
        }
        var tile = new Border
        {
            Width = 56,
            Height = 56,
            CornerRadius = new CornerRadius(4),
            ClipToBounds = true,
            BorderThickness = new Thickness(active ? 2 : 1),
            BorderBrush = active ? Brushes.White : new SolidColorBrush(Colors.Gray, 0.6),
            Child = image,
        };
        var button = new Button
        {
            Width = 56,
            Height = 56,
            Padding = new Thickness(0),
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Content = tile,
        };
        button.Click += (_, _) => SelectThumbnail(index);
        return button;
    }

    private static Button NavButton(string glyph, string tip, Action onClick, HorizontalAlignment align)
    {
        var button = new Button
        {
            Content = new TextBlock { Text = glyph, FontSize = 20, Foreground = Brushes.White },
            Width = 40,
            Height = 40,
            CornerRadius = new CornerRadius(20),
            HorizontalContentAlignment = HorizontalAlignment.Center,
            VerticalContentAlignment = VerticalAlignment.Center,
            HorizontalAlignment = align,
            Background = new SolidColorBrush(Colors.Black, 0.4),
        };
        ToolTip.SetTip(button, tip);
        Avalonia.Automation.AutomationProperties.SetName(button, tip);
        button.Click += (_, _) => onClick();
        return button;
    }

    private static TextBlock ChromeText(double size) => new()
    {
        FontSize = size,
        Foreground = Brushes.White,
        VerticalAlignment = VerticalAlignment.Center,
    };

    private static Bitmap? Decode(byte[]? bytes)
    {
        if (bytes is null)
        {
            return null;
        }
        try
        {
            using var stream = new System.IO.MemoryStream(bytes);
            return new Bitmap(stream);
        }
        catch (Exception)
        {
            return null;
        }
    }
}
