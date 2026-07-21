using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.Globalization;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading.Tasks;
using Microsoft.UI;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Foundation;
using Windows.UI;

namespace Bae.Windows;

// The queue side pane: a non-modal surface hosting the manual "Up Next" lane and
// the playback context as ONE virtualized, reorderable ListView (never a ListView
// inside a StackPanel inside a ScrollViewer — that combination hands the ListView
// unbounded height, so ItemsStackPanel realizes every row instead of
// virtualizing). Non-modal so the album grid behind it can source a drag while
// the pane stays open — album cards drop into the manual lane at a chosen index.
// Reads the lanes from the playback store and rebuilds the list on its
// QueueChanged while visible; each row skips on click, removes on right-tap or its
// remove button, and reorders on drag (forwarded to core by entry id). Clear
// empties only the manual lane. External drops land only in the manual lane; the
// context lane rejects them. The context lane is library-scaled and only partly
// resolved: it pages further entries in via ISupportIncrementalLoading as the list
// scrolls toward its end, revision-checked against the store so a reply for a
// superseded queue state is dropped.
//
// The header and the now-playing card are fixed chrome that survive list rebuilds:
// the shell (header / card / list slot) is built once when the pane opens, and a
// QueueChanged only swaps a fresh list into the slot. The card renders the same
// now-playing track the bar shows, driven by the store's now-playing and position
// events (subscribed for the pane's whole life), not by QueueChanged.
internal sealed class QueuePane
{
    private readonly SessionStore _session;
    private readonly PlaybackStore _playback;
    private readonly Border _host;
    private readonly Action<string> _onError;

    // Shell chrome, built once per open and held so events update it in place. Null
    // while the pane is closed, which is the guard the now-playing handlers check
    // before touching any visual.
    private Button? _clearButton;
    private ContentControl? _listSlot;
    private Border? _card;
    private Image? _cardCover;
    private TextBlock? _cardTitle;
    private TextBlock? _cardArtist;
    private TextBlock? _cardElapsed;
    private ColumnDefinition? _cardProgressFill;
    private ColumnDefinition? _cardProgressRest;

    // The latest now-playing snapshot the store has emitted, cached for the whole
    // pane life so opening the pane can render the card immediately (no event fires
    // just because the pane opened). Null while playback is stopped.
    private NowPlayingBarTrack? _nowPlaying;
    private PlaybackPositionRender? _position;

    // The whole second the card's elapsed readout last rendered, so position ticks
    // (several per second) collapse to one update per second — it is a readout, not
    // a scrubber.
    private long _renderedSecond = -1;

    public QueuePane(SessionStore session, PlaybackStore playback, Border host, Action<string> onError)
    {
        _session = session;
        _playback = playback;
        _host = host;
        _onError = onError;

        // The card follows the same now-playing state the bar renders. Subscribed
        // for the pane's whole life (cheap) so the cache is warm when the pane
        // opens; the handlers no-op on the visuals while the pane is closed.
        _playback.NowPlayingChanged += OnNowPlayingChanged;
        _playback.PlaybackStopped += OnPlaybackStopped;
        _playback.PositionChanged += OnPositionChanged;
    }

    // -- Row model ---------------------------------------------------------
    //
    // The pane's single ListView is bound to one flat, heterogeneous row
    // collection. Every row carries the lane it belongs to; the collection's
    // rows are always laid out as contiguous same-lane runs in `Lane` ordinal
    // order (Manual, then Context) — reorder validation leans on that invariant
    // instead of tracking section boundaries separately.
    private enum QueueLane
    {
        Manual,
        Context,
    }

    private abstract record QueueRow(QueueLane Lane);

    // A section header. Shuffled is non-null only for the context section (the
    // manual lane is never shuffled and has no toggle).
    private sealed record SectionHeaderRow(QueueLane Lane, string Text, bool? Shuffled) : QueueRow(Lane);

    // One queue row's display: the entry and its lane.
    private sealed record EntryRow(QueueLane Lane, BridgeQueueEntry Entry) : QueueRow(Lane)
    {
        internal string EntryId => Entry.EntryId;
    }

    // The empty manual-lane state: shown instead of any manual EntryRow when the
    // lane is empty, so a drop always has a target at the front of the pane.
    private sealed record EmptyManualRow() : QueueRow(QueueLane.Manual);

    // A trailing drop strip appended after a non-empty manual lane, mirroring the
    // reference platforms' "drop here to append" zone. Belongs to Manual — it
    // sits inside that lane's contiguous run, immediately before the context
    // section (or the end of the list).
    private sealed record TrailingDropRow() : QueueRow(QueueLane.Manual);

    public bool IsOpen => _host.Visibility == Visibility.Visible;

    // Show the pane if hidden, hide it if shown. Bound to the queue button and its
    // Ctrl+Shift+Q accelerator.
    public void Toggle()
    {
        if (IsOpen)
        {
            Hide();
        }
        else
        {
            Show();
        }
    }

    public void Hide()
    {
        if (!IsOpen)
        {
            return;
        }
        _playback.QueueChanged -= Rebuild;
        _host.Child = null;
        _clearButton = null;
        _listSlot = null;
        _card = null;
        _cardCover = null;
        _cardTitle = null;
        _cardArtist = null;
        _cardElapsed = null;
        _cardProgressFill = null;
        _cardProgressRest = null;
        _host.Visibility = Visibility.Collapsed;
    }

    private void Show()
    {
        if (_session.CurrentHandleOrNull() == null)
        {
            return;
        }
        _host.Child = BuildShell();
        Rebuild();
        RenderCard();
        _playback.QueueChanged += Rebuild;
        _host.Visibility = Visibility.Visible;
    }

    // The pane's fixed chrome: a header, the now-playing card, and a slot the list
    // is swapped into on each rebuild. Built once per open.
    private Grid BuildShell()
    {
        var shell = new Grid();
        shell.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        shell.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        shell.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });

        var header = BuildHeader();
        Grid.SetRow(header, 0);
        shell.Children.Add(header);

        var card = BuildNowPlayingCard();
        Grid.SetRow(card, 1);
        shell.Children.Add(card);

        _listSlot = new ContentControl
        {
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            VerticalContentAlignment = VerticalAlignment.Stretch,
        };
        Grid.SetRow(_listSlot, 2);
        shell.Children.Add(_listSlot);

        return shell;
    }

    // Rebuild the pane's list from the current lanes. Safe to run mid-drag: core
    // emits QueueChanged only after a mutation lands, and the list is rebuilt
    // whole between events. QueueChanged is also the invalidation signal for any
    // in-flight incremental-load page fetch on the PREVIOUS collection instance:
    // this always constructs a fresh QueuePaneRowCollection, so a stale reply lands
    // on an abandoned instance nobody renders. The header and card are untouched —
    // only Clear's enablement tracks the manual lane.
    private void Rebuild()
    {
        var manual = _playback.ManualQueue;
        var context = _playback.Context;
        var revision = _playback.Revision;

        var rows = new QueuePaneRowCollection(_session, revision, contextTotal: context?.UpcomingTotal ?? 0);

        // "Up Next" labels the manual lane only when it has rows (an empty lane
        // shows its drop card without a heading, matching macOS).
        if (manual.Count == 0)
        {
            rows.Add(new EmptyManualRow());
        }
        else
        {
            rows.Add(new SectionHeaderRow(QueueLane.Manual, Loc.Chrome("queue.section.up_next"), Shuffled: null));
            foreach (var entry in manual)
            {
                rows.Add(new EntryRow(QueueLane.Manual, entry));
            }
            rows.Add(new TrailingDropRow());
        }

        if (context is { UpcomingTotal: > 0 } ctx)
        {
            rows.Add(new SectionHeaderRow(QueueLane.Context, ContextSectionTitle(ctx), ctx.Shuffled));
            foreach (var entry in ctx.Upcoming)
            {
                rows.Add(new EntryRow(QueueLane.Context, entry));
            }
            rows.MarkInitialContextLoad((ulong)ctx.Upcoming.Length);
        }

        _listSlot!.Content = BuildQueueList(rows, manual);
        if (_clearButton is not null)
        {
            _clearButton.IsEnabled = manual.Count > 0;
        }
    }

    // The context section's title: the whole library, or "Playing From" a release —
    // with the release title appended ("Playing From · {title}") when the context
    // carries one. The localized words are the UI's; the composition mirrors macOS.
    private static string ContextSectionTitle(BridgePlaybackContext context)
    {
        if (context.Kind == BridgePlaybackSourceKind.Library)
        {
            return Loc.Chrome("queue.section.your_library");
        }
        var playingFrom = Loc.Chrome("queue.section.playing_from");
        return string.IsNullOrEmpty(context.SourceTitle)
            ? playingFrom
            : $"{playingFrom} · {context.SourceTitle}";
    }

    // The pane header: the "queue" title, a Clear button that empties the manual
    // lane (disabled while it is empty), and a close button that hides the pane.
    private Grid BuildHeader()
    {
        var title = new TextBlock
        {
            Text = Loc.Chrome("queue.title"),
            FontSize = 22,
            FontWeight = FontWeights.ExtraBold,
            VerticalAlignment = VerticalAlignment.Center,
        };

        _clearButton = new Button
        {
            Content = Loc.Chrome("queue.clear"),
            Background = new SolidColorBrush(Colors.Transparent),
            BorderThickness = new Thickness(0),
            Foreground = Secondary,
            FontSize = 12,
            FontWeight = FontWeights.SemiBold,
            Padding = new Thickness(6, 4, 6, 4),
            VerticalAlignment = VerticalAlignment.Center,
            IsEnabled = _playback.ManualQueue.Count > 0,
        };
        AutomationName(_clearButton, "queue.clear");
        _clearButton.Click += (_, _) => _session.WithCurrentHandle(NativeBae.QueueClear);

        var close = new Button
        {
            // Segoe MDL2 Assets "ChromeClose" glyph (U+E711).
            Content = new FontIcon { Glyph = "\uE711", FontSize = 14 },
            Background = new SolidColorBrush(Colors.Transparent),
            BorderThickness = new Thickness(0),
            VerticalAlignment = VerticalAlignment.Center,
        };
        AutomationName(close, "action.close");
        close.Click += (_, _) => Hide();

        var grid = new Grid { Padding = new Thickness(20, 18, 12, 12) };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(title, 0);
        Grid.SetColumn(_clearButton, 1);
        Grid.SetColumn(close, 2);
        grid.Children.Add(title);
        grid.Children.Add(_clearButton);
        grid.Children.Add(close);
        return grid;
    }

    // The now-playing card: fixed chrome between the header and the list, rendering
    // the same track the bar shows. Not a ListView row — building it here keeps the
    // list's virtualization untouched. Hidden until RenderCard shows it.
    private Border BuildNowPlayingCard()
    {
        _cardCover = new Image { Stretch = Stretch.UniformToFill };
        var art = new Border
        {
            Width = 56,
            Height = 56,
            CornerRadius = new CornerRadius(9),
            Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"],
            Child = _cardCover,
            Translation = new System.Numerics.Vector3(0, 0, 8),
        };
        art.Shadow = new ThemeShadow();

        var eyebrow = new TextBlock
        {
            Text = Loc.Chrome("queue.now_playing").ToUpper(CultureInfo.CurrentUICulture),
            FontSize = 9,
            FontWeight = FontWeights.Bold,
            CharacterSpacing = 130,
            Foreground = Accent,
        };
        _cardTitle = new TextBlock
        {
            FontSize = 14,
            FontWeight = FontWeights.Bold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        _cardArtist = new TextBlock
        {
            FontSize = 11,
            FontWeight = FontWeights.Medium,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
            Foreground = Secondary,
        };

        var text = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
        text.Children.Add(eyebrow);
        text.Children.Add(_cardTitle);
        text.Children.Add(_cardArtist);
        text.Children.Add(BuildCardProgress());

        var row = new Grid();
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        Grid.SetColumn(art, 0);
        Grid.SetColumn(text, 1);
        art.Margin = new Thickness(0, 0, 12, 0);
        row.Children.Add(art);
        row.Children.Add(text);

        _card = new Border
        {
            Margin = new Thickness(14, 0, 14, 6),
            Padding = new Thickness(12),
            CornerRadius = new CornerRadius(12),
            BorderThickness = new Thickness(1),
            BorderBrush = ForegroundWash(0.08),
            Background = CardWashBrush(),
            Visibility = Visibility.Collapsed,
            Child = row,
        };
        return _card;
    }

    // The card's progress strip: a display-only 4px track with an accent-gradient
    // fill and a fixed-width elapsed readout. No seek — it mirrors the store's
    // position.
    private Grid BuildCardProgress()
    {
        _cardProgressFill = new ColumnDefinition { Width = new GridLength(0, GridUnitType.Star) };
        _cardProgressRest = new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) };

        var fill = new Border
        {
            CornerRadius = new CornerRadius(2),
            Background = ProgressFillBrush(),
        };
        var trackGrid = new Grid();
        trackGrid.ColumnDefinitions.Add(_cardProgressFill);
        trackGrid.ColumnDefinitions.Add(_cardProgressRest);
        Grid.SetColumn(fill, 0);
        trackGrid.Children.Add(fill);

        var track = new Border
        {
            Height = 4,
            CornerRadius = new CornerRadius(2),
            Background = ForegroundWash(0.12),
            VerticalAlignment = VerticalAlignment.Center,
            Child = trackGrid,
        };

        _cardElapsed = new TextBlock
        {
            Text = "0:00",
            Width = 34,
            TextAlignment = TextAlignment.Right,
            FontSize = 10,
            FontWeight = FontWeights.SemiBold,
            Foreground = Secondary,
            VerticalAlignment = VerticalAlignment.Center,
        };

        var strip = new Grid { Margin = new Thickness(0, 6, 0, 0), ColumnSpacing = 8 };
        strip.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        strip.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(track, 0);
        Grid.SetColumn(_cardElapsed, 1);
        strip.Children.Add(track);
        strip.Children.Add(_cardElapsed);
        return strip;
    }

    // -- Now-playing card rendering ---------------------------------------

    private void OnNowPlayingChanged(NowPlayingBarTrack track)
    {
        _nowPlaying = track;
        RenderCard();
    }

    private void OnPlaybackStopped()
    {
        _nowPlaying = null;
        _position = null;
        RenderCard();
    }

    private void OnPositionChanged(PlaybackPositionRender render)
    {
        _position = render;
        RenderCardPosition(force: false);
    }

    // Render the card from the cached now-playing track: show it with the track's
    // art/title/artist while playback is active, hide it when stopped. No-op while
    // the pane is closed (the card chrome doesn't exist).
    private void RenderCard()
    {
        if (_card is null)
        {
            return;
        }
        if (_nowPlaying is { } track)
        {
            _card.Visibility = Visibility.Visible;
            _cardTitle!.Text = track.Title;
            _cardArtist!.Text = track.Artist;
            _cardCover!.Source = CoverImage.LoadImage(_session.CurrentHandleOrNull(), track.CoverImageId);
            _renderedSecond = -1;
            RenderCardPosition(force: true);
        }
        else
        {
            _card.Visibility = Visibility.Collapsed;
        }
    }

    // Update the progress fill and elapsed readout, collapsed to one update per
    // second (a readout, not a scrubber). `force` re-renders on a track change even
    // within the same second.
    private void RenderCardPosition(bool force)
    {
        if (_card is null || _nowPlaying is null || _position is not { } position)
        {
            return;
        }
        var second = (long)(position.PositionMs / 1000);
        if (!force && second == _renderedSecond)
        {
            return;
        }
        _renderedSecond = second;
        var progress = Math.Clamp(position.Progress, 0, 1);
        _cardProgressFill!.Width = new GridLength(progress, GridUnitType.Star);
        _cardProgressRest!.Width = new GridLength(1 - progress, GridUnitType.Star);
        _cardElapsed!.Text = BridgeDisplay.Clock(position.PositionMs);
    }

    // The single virtualized list hosting every row: both section headers and both
    // lanes' entries. Fills the pane's remaining height (its slot's Grid row is
    // Star-sized) so it is its own bounded scroll region — there is no outer
    // ScrollViewer.
    private ListView BuildQueueList(QueuePaneRowCollection rows, IReadOnlyList<BridgeQueueEntry> manual)
    {
        var list = new ListView
        {
            ItemsSource = rows,
            SelectionMode = ListViewSelectionMode.None,
            IsItemClickEnabled = true,
            CanReorderItems = true,
            CanDragItems = true,
            AllowDrop = true,
            Padding = new Thickness(12, 4, 12, 16),
            IncrementalLoadingTrigger = IncrementalLoadingTrigger.Edge,
        };

        // The custom rows carry their own hover fill; neutralize the container's
        // default pointer/selection backgrounds so the two don't stack, and strip
        // its padding/min-height so a row is exactly its content.
        foreach (var key in new[]
        {
            "ListViewItemBackgroundPointerOver",
            "ListViewItemBackgroundPressed",
            "ListViewItemBackgroundSelected",
            "ListViewItemBackgroundSelectedPointerOver",
            "ListViewItemBackgroundSelectedPressed",
        })
        {
            list.Resources[key] = new SolidColorBrush(Colors.Transparent);
        }
        var itemStyle = new Style(typeof(ListViewItem));
        itemStyle.Setters.Add(new Setter(Control.PaddingProperty, new Thickness(0)));
        itemStyle.Setters.Add(new Setter(FrameworkElement.MinHeightProperty, 0.0));
        itemStyle.Setters.Add(new Setter(Control.HorizontalContentAlignmentProperty, HorizontalAlignment.Stretch));
        list.ItemContainerStyle = itemStyle;

        list.ContainerContentChanging += (_, args) =>
        {
            if (args.InRecycleQueue)
            {
                return;
            }
            args.ItemContainer.Content = BuildRowVisual(args.Item as QueueRow, rows);
            args.Handled = true;
        };

        // Only an EntryRow is draggable — headers, the empty state, and the
        // trailing drop strip are chrome, not queue content.
        list.DragItemsStarting += (_, args) =>
        {
            if (args.Items.Any(item => item is not EntryRow))
            {
                args.Cancel = true;
            }
        };

        list.ItemClick += (_, args) =>
        {
            if (args.ClickedItem is EntryRow clicked)
            {
                _session.WithCurrentHandle(handle => NativeBae.QueueSkipTo(handle, clicked.EntryId));
            }
        };

        // Right-tap a row to drop it from the queue. Removing locally too keeps
        // the pane in sync; the Move-only reorder handler below ignores this
        // Remove (it only reacts to NotifyCollectionChangedAction.Move).
        list.RightTapped += (_, args) =>
        {
            if (args.OriginalSource is not FrameworkElement element
                || element.DataContext is not EntryRow item)
            {
                return;
            }

            if (rows.IndexOf(item) < 0)
            {
                return;
            }

            var menu = new MenuFlyout();
            var remove = new MenuFlyoutItem { Text = Loc.Chrome("queue.remove_item") };
            remove.Click += (_, _) => RemoveEntry(rows, item);
            menu.Items.Add(remove);
            menu.ShowAt(element, new FlyoutShowOptions { Position = args.GetPosition(element) });
        };

        // A drag-reorder mutates `rows` in place before this fires (the framework's
        // own behavior); validate it stayed within one lane before forwarding to
        // core, and revert (a full Rebuild, since core's state never changed) if
        // it crossed a section boundary — dragging past a header is visually
        // possible but not a valid move.
        rows.CollectionChanged += (_, args) =>
        {
            if (args.Action != NotifyCollectionChangedAction.Move)
            {
                return;
            }
            if (rows[args.NewStartingIndex] is not EntryRow moved)
            {
                return;
            }
            if (!IsLaneOrderValid(rows))
            {
                BaeDiagnostics.Logger.Warning(
                    $"Queue reorder for {moved.EntryId} crossed a section boundary; reverting.");
                Rebuild();
                return;
            }

            var laneEntryIds = rows.OfType<EntryRow>().Where(e => e.Lane == moved.Lane)
                .Select(e => e.EntryId).ToList();
            var newIndex = laneEntryIds.IndexOf(moved.EntryId);
            var move = QueueReorderModel.ResolveMove(laneEntryIds, newIndex);
            _session.WithCurrentHandle(handle => NativeBae.QueueReorder(handle, move.MovedEntryId, move.BeforeEntryId));
        };

        // External album-card drops land only in the manual lane; the pointer's Y
        // position resolves to an insert index among the manual lane's realized
        // rows (or the lane's end when it's empty or the pointer is past them).
        AttachExternalDrop(list, e => ComputeInsertIndex(list, rows, manual, e));

        return list;
    }

    // Remove one entry from core and from the live collection. The bridge removes
    // by entry id, so a reorder between the gesture and this can't target the wrong
    // row; the local index only keeps the pane's collection in sync.
    private void RemoveEntry(QueuePaneRowCollection rows, EntryRow item)
    {
        var index = rows.IndexOf(item);
        if (index < 0)
        {
            return;
        }
        _session.WithCurrentHandle(handle => NativeBae.QueueRemove(handle, item.EntryId));
        rows.RemoveAt(index);
    }

    // Whether `rows` still partitions into contiguous same-lane runs in
    // Manual/Context ordinal order — the invariant a valid single-lane reorder
    // always preserves. A cross-lane drag breaks it (some row's ordinal would
    // regress relative to an earlier row's).
    private static bool IsLaneOrderValid(IReadOnlyList<QueueRow> rows)
    {
        var lastOrdinal = -1;
        foreach (var row in rows)
        {
            var ordinal = (int)row.Lane;
            if (ordinal < lastOrdinal)
            {
                return false;
            }
            lastOrdinal = ordinal;
        }
        return true;
    }

    // Build one row's visual by kind. ContainerContentChanging re-fires per
    // recycled container, so this runs again whenever a row's content changes
    // (including a fresh page landing).
    private FrameworkElement BuildRowVisual(QueueRow? row, QueuePaneRowCollection rows) => row switch
    {
        SectionHeaderRow { Shuffled: null } header => QueueSectionLabel(header.Text),
        SectionHeaderRow header => ContextSectionLabel(header.Text, header.Shuffled!.Value),
        EntryRow entry => BuildEntryRow(entry, rows),
        EmptyManualRow => BuildEmptyDropArea(),
        TrailingDropRow => BuildTrailingDropArea(),
        _ => throw new ArgumentOutOfRangeException(nameof(row), row, "Unknown queue row kind"),
    };

    // The empty manual-lane state: a heading and a hint centered inside a
    // drop-accepting card; a drop lands at the front of the (empty) lane.
    private Border BuildEmptyDropArea()
    {
        var stack = new StackPanel
        {
            Spacing = 6,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        stack.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("queue.empty.title"),
            HorizontalAlignment = HorizontalAlignment.Center,
            FontWeight = FontWeights.SemiBold,
        });
        stack.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("queue.empty.hint"),
            HorizontalAlignment = HorizontalAlignment.Center,
            TextWrapping = TextWrapping.Wrap,
            TextAlignment = TextAlignment.Center,
            Foreground = Secondary,
        });
        return new Border
        {
            MinHeight = 120,
            Padding = new Thickness(16),
            Child = stack,
        };
    }

    // A drop strip below the manual lane's rows that appends at the lane's end,
    // mirroring the reference platforms' trailing zone.
    private static Border BuildTrailingDropArea() => new()
    {
        Height = 18,
        Background = new SolidColorBrush(Colors.Transparent),
    };

    // A plain section header for the manual "Up Next" lane (never shuffled, no
    // shuffle control): a small, wide-tracked, uppercase secondary label.
    private static TextBlock QueueSectionLabel(string text) => new()
    {
        Text = text.ToUpper(CultureInfo.CurrentUICulture),
        FontSize = 10,
        FontWeight = FontWeights.Bold,
        CharacterSpacing = 120,
        Foreground = Secondary,
        Margin = new Thickness(8, 12, 8, 4),
    };

    // The context section's header: the same label style plus a shuffle toggle
    // that flips the context between sequential and shuffled order while the
    // current track keeps playing.
    private Grid ContextSectionLabel(string text, bool shuffled)
    {
        var label = new TextBlock
        {
            Text = text.ToUpper(CultureInfo.CurrentUICulture),
            FontSize = 10,
            FontWeight = FontWeights.Bold,
            CharacterSpacing = 120,
            Foreground = Secondary,
            TextTrimming = TextTrimming.CharacterEllipsis,
            MaxLines = 1,
            VerticalAlignment = VerticalAlignment.Center,
        };

        // Segoe MDL2 Assets "Shuffle" glyph (U+E8B1): accent on a soft-accent fill
        // when on, secondary when off.
        var toggle = new Button
        {
            Content = new FontIcon
            {
                Glyph = "",
                FontSize = 12,
                Foreground = shuffled ? Accent : Secondary,
            },
            Width = 28,
            Height = 28,
            Padding = new Thickness(0),
            CornerRadius = new CornerRadius(7),
            BorderThickness = new Thickness(0),
            Background = shuffled ? AccentSoftFill() : new SolidColorBrush(Colors.Transparent),
            VerticalAlignment = VerticalAlignment.Center,
        };
        var shuffleKey = shuffled ? "queue.shuffle.off" : "queue.shuffle.on";
        AutomationName(toggle, shuffleKey);
        toggle.Click += (_, _) => _session.WithCurrentHandle(handle => NativeBae.SetShuffle(handle, !shuffled));

        var row = new Grid { Margin = new Thickness(8, 12, 8, 4) };
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(label, 0);
        Grid.SetColumn(toggle, 1);
        row.Children.Add(label);
        row.Children.Add(toggle);
        return row;
    }

    // A queue entry row: cover, title/album, and a trailing slot that swaps the
    // duration for a remove button on hover. The swap is opacity-only in a
    // fixed-size slot, so the row never resizes on hover.
    private Border BuildEntryRow(EntryRow row, QueuePaneRowCollection rows)
    {
        var cover = new Image { Stretch = Stretch.UniformToFill };
        CoverImage.BindById(cover, _session.CurrentHandleOrNull(), row.Entry.CoverImageId);
        var art = new Border
        {
            Width = 44,
            Height = 44,
            CornerRadius = new CornerRadius(8),
            Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"],
            Child = cover,
        };

        var title = new TextBlock
        {
            Text = row.Entry.Title,
            FontSize = 13,
            FontWeight = FontWeights.SemiBold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        var album = new TextBlock
        {
            Text = row.Entry.AlbumTitle,
            FontSize = 11,
            FontWeight = FontWeights.Medium,
            Foreground = Secondary,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        var textColumn = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
        textColumn.Children.Add(title);
        textColumn.Children.Add(album);

        var duration = new TextBlock
        {
            Text = BridgeDisplay.Clock(row.Entry.DurationMs),
            FontSize = 11,
            FontWeight = FontWeights.SemiBold,
            Foreground = Secondary,
            VerticalAlignment = VerticalAlignment.Center,
            HorizontalAlignment = HorizontalAlignment.Right,
        };
        var removeGlyph = new FontIcon { Glyph = "\uE711", FontSize = 11, Foreground = Secondary };
        var remove = new Button
        {
            Content = removeGlyph,
            Width = 28,
            Height = 28,
            Padding = new Thickness(0),
            CornerRadius = new CornerRadius(7),
            BorderThickness = new Thickness(0),
            Background = new SolidColorBrush(Colors.Transparent),
            Opacity = 0,
            IsHitTestVisible = false,
            VerticalAlignment = VerticalAlignment.Center,
            HorizontalAlignment = HorizontalAlignment.Right,
        };
        AutomationName(remove, "queue.remove_item");
        remove.PointerEntered += (_, _) =>
        {
            removeGlyph.Foreground = Accent;
            remove.Background = AccentSoftFill();
        };
        remove.PointerExited += (_, _) =>
        {
            removeGlyph.Foreground = Secondary;
            remove.Background = new SolidColorBrush(Colors.Transparent);
        };
        remove.Click += (_, _) => RemoveEntry(rows, row);

        var trailing = new Grid { VerticalAlignment = VerticalAlignment.Center };
        trailing.Children.Add(duration);
        trailing.Children.Add(remove);

        var grid = new Grid { ColumnSpacing = 12, VerticalAlignment = VerticalAlignment.Center };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(art, 0);
        Grid.SetColumn(textColumn, 1);
        Grid.SetColumn(trailing, 2);
        grid.Children.Add(art);
        grid.Children.Add(textColumn);
        grid.Children.Add(trailing);

        var hover = new Border
        {
            Padding = new Thickness(8, 6, 8, 6),
            Margin = new Thickness(0, 1, 0, 1),
            CornerRadius = new CornerRadius(10),
            Background = new SolidColorBrush(Colors.Transparent),
            Child = grid,
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(hover, $"{row.Entry.Title}, {row.Entry.AlbumTitle}");
        hover.PointerEntered += (_, _) =>
        {
            hover.Background = ForegroundWash(0.06);
            duration.Opacity = 0;
            remove.Opacity = 1;
            remove.IsHitTestVisible = true;
        };
        hover.PointerExited += (_, _) =>
        {
            hover.Background = new SolidColorBrush(Colors.Transparent);
            duration.Opacity = 1;
            remove.Opacity = 0;
            remove.IsHitTestVisible = false;
        };
        return hover;
    }

    // Wire a surface to accept an external album-card drop into the manual lane.
    // The Text discriminator separates external drags (which carry the album-id
    // payload) from the framework's internal reorder (which carries no Text), so
    // reorder is left untouched. The caption previews the 1-based target position.
    // e.Handled is required: DragOver/Drop bubble, and the window's folder-drop
    // handler would otherwise overwrite the accepted operation for a non-file drag.
    private void AttachExternalDrop(UIElement target, Func<DragEventArgs, int> computeIndex)
    {
        target.AllowDrop = true;
        target.DragOver += (_, e) =>
        {
            if (!e.DataView.Contains(StandardDataFormats.Text))
            {
                return;
            }
            e.AcceptedOperation = DataPackageOperation.Copy;
            if (e.DragUIOverride is not null)
            {
                e.DragUIOverride.Caption = Loc.Chrome("queue.drop_insert_caption", "position", computeIndex(e) + 1);
            }
            e.Handled = true;
        };
        target.Drop += async (_, e) =>
        {
            if (!e.DataView.Contains(StandardDataFormats.Text))
            {
                return;
            }
            e.Handled = true;
            var index = computeIndex(e);
            var ids = await ReadDropPayload(e);
            if (ids is null)
            {
                return;
            }
            await ResolveAndApply(ids, (handle, trackIds) =>
            {
                NativeBae.InsertInQueue(handle, trackIds, index);
                return null;
            });
        };
    }

    // Append a card dropped on the queue button to the end of the manual lane.
    // Shares the payload read and resolve path with the in-pane drops; the queue
    // button works whether or not the pane is open.
    public async Task HandleButtonAppendDrop(DragEventArgs e)
    {
        var ids = await ReadDropPayload(e);
        if (ids is null)
        {
            return;
        }
        await ResolveAndApply(ids, (handle, trackIds) => NativeBae.AddToQueue(handle, trackIds));
    }

    // The album grid's bulk Add to Queue / Play Next: resolves album ids to
    // tracks and applies them, sharing the drag-drop resolve-then-apply route
    // and its error banner (the same route macOS's QueueActions gives both the
    // now-playing-bar drop and the grid's bulk menu).
    public Task AddAlbumsToQueue(IReadOnlyList<string> albumIds, bool addNext) =>
        ResolveAndApply(albumIds, (handle, trackIds) => addNext
            ? NativeBae.AddNext(handle, trackIds)
            : NativeBae.AddToQueue(handle, trackIds));

    // Read and decode the drag payload, releasing the drop as soon as the data is
    // read. Null when the payload carries no ids (a Text drag that isn't ours).
    private static async Task<IReadOnlyList<string>?> ReadDropPayload(DragEventArgs e)
    {
        var deferral = e.GetDeferral();
        string payload;
        try
        {
            payload = await e.DataView.GetTextAsync();
        }
        finally
        {
            deferral.Complete();
        }

        var ids = QueueDragPayload.Decode(payload);
        if (ids.Count == 0)
        {
            BaeDiagnostics.Logger.Warning("Ignoring queue drop with no ids in its payload.");
            return null;
        }
        return ids;
    }

    // The manual-lane index a drop at the pointer inserts before: the position of
    // the first realized manual row whose midpoint is below the pointer, else the
    // lane's end. Realized indices are resolved against the flat row collection and
    // translated back to a manual-lane-relative index.
    private static int ComputeInsertIndex(
        ListView list, QueuePaneRowCollection rows, IReadOnlyList<BridgeQueueEntry> manual, DragEventArgs e)
    {
        var pointerY = e.GetPosition(list).Y;
        var realizedRows = new List<RealizedRow>();
        var manualIndex = 0;
        for (var flatIndex = 0; flatIndex < rows.Count; flatIndex++)
        {
            if (rows[flatIndex] is not EntryRow { Lane: QueueLane.Manual })
            {
                continue;
            }
            if (list.ContainerFromIndex(flatIndex) is FrameworkElement container)
            {
                var top = container.TransformToVisual(list).TransformPoint(new Point(0, 0)).Y;
                realizedRows.Add(new RealizedRow(manualIndex, top + container.ActualHeight / 2));
            }
            manualIndex++;
        }
        return QueueDropIndex.Insert(realizedRows, pointerY, manual.Count);
    }

    // Resolve the dragged album/track ids to track ids and hand them to apply
    // (insert at an index, or append), which returns an error message or null.
    // Resolve and apply run off the UI thread; a resolve failure surfaces in the
    // pane's error banner, and an empty resolve is logged and dropped (the core
    // clamps the index, so a queue mutation racing the drop degrades to a clamped
    // insert).
    private async Task ResolveAndApply(IReadOnlyList<string> ids, Func<AppHandle, IReadOnlyList<string>, string?> apply)
    {
        var outcome = await Task.Run(() =>
        {
            var (current, resolved) = _session.WithCurrentHandle(handle => NativeBae.ResolveToTrackIds(handle, ids));
            if (!current)
            {
                return (Current: false, Error: (string?)null, Empty: false);
            }
            var (trackIds, error) = resolved;
            if (error is not null)
            {
                return (Current: true, Error: error, Empty: false);
            }
            if (trackIds is null || trackIds.Count == 0)
            {
                return (Current: true, Error: (string?)null, Empty: true);
            }
            var applyError = _session.WithCurrentHandle(handle => apply(handle, trackIds));
            return (Current: true, Error: applyError.Result, Empty: false);
        });

        if (!outcome.Current)
        {
            return;
        }
        if (outcome.Error is not null)
        {
            _onError(outcome.Error);
            return;
        }
        if (outcome.Empty)
        {
            BaeDiagnostics.Logger.Warning($"Queue drop resolved no tracks for ids [{string.Join(", ", ids)}].");
        }
    }

    // -- Brushes -----------------------------------------------------------
    //
    // macOS's fixed dark palette maps to theme-aware Fluent brushes: accent = the
    // system accent, secondary/tertiary text = the TextFillColor steps, soft fills
    // = the accent color at low opacity, neutral washes = the foreground color at
    // low opacity. Freshly-constructed brushes because a brush instance can't be
    // shared across elements once parented.
    private static Brush Accent => (Brush)Application.Current.Resources["AccentTextFillColorPrimaryBrush"];

    private static Brush Secondary => (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"];

    private static SolidColorBrush AccentSoftFill() =>
        new((Color)Application.Current.Resources["SystemAccentColor"]) { Opacity = 0.22 };

    private static SolidColorBrush ForegroundWash(double opacity) =>
        new((Color)Application.Current.Resources["TextFillColorPrimary"]) { Opacity = opacity };

    // The now-playing card's fill: a faint accent wash over a subtle card surface,
    // top-leading to bottom-trailing.
    private static LinearGradientBrush CardWashBrush()
    {
        var accent = (Color)Application.Current.Resources["SystemAccentColor"];
        var brush = new LinearGradientBrush { StartPoint = new Point(0, 0), EndPoint = new Point(1, 1) };
        brush.GradientStops.Add(new GradientStop { Color = Color.FromArgb(10, accent.R, accent.G, accent.B), Offset = 0 });
        brush.GradientStops.Add(new GradientStop
        {
            Color = (Color)Application.Current.Resources["CardBackgroundFillColorSecondary"],
            Offset = 1,
        });
        return brush;
    }

    // The progress fill: a horizontal accent gradient, accent to a lightened accent
    // (the same pair the bar's scrubber uses).
    private static LinearGradientBrush ProgressFillBrush()
    {
        var brush = new LinearGradientBrush { StartPoint = new Point(0, 0), EndPoint = new Point(1, 0) };
        brush.GradientStops.Add(new GradientStop { Color = (Color)Application.Current.Resources["SystemAccentColor"], Offset = 0 });
        brush.GradientStops.Add(new GradientStop { Color = (Color)Application.Current.Resources["SystemAccentColorLight2"], Offset = 1 });
        return brush;
    }

    private static void AutomationName(Button button, string key)
    {
        var label = Loc.Chrome(key);
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(button, label);
        ToolTipService.SetToolTip(button, label);
    }

    // The flat row collection backing the pane's single ListView. Paging is
    // append-only (WinUI's incremental-loading contract only ever asks to load
    // more at the END of the bound collection), which lines up with the context
    // section always being the tail-most section: "near the end of the whole
    // list" is exactly "near the end of the context tail." A reply is merged only
    // if its revision still matches the revision this instance was built for;
    // Rebuild always constructs a fresh instance on every QueueChanged, so a
    // stale in-flight reply lands on an abandoned collection nobody renders.
    private sealed class QueuePaneRowCollection : ObservableCollection<QueueRow>, ISupportIncrementalLoading
    {
        private readonly SessionStore _session;
        private readonly ulong _revision;
        private readonly ulong _contextTotal;
        private ulong _contextLoadedCount;

        public QueuePaneRowCollection(SessionStore session, ulong revision, ulong contextTotal)
        {
            _session = session;
            _revision = revision;
            _contextTotal = contextTotal;
        }

        // Called once from Rebuild after seeding the initial window, so
        // HasMoreItems reflects what's already loaded before any paging.
        public void MarkInitialContextLoad(ulong loadedCount) => _contextLoadedCount = loadedCount;

        public bool HasMoreItems => _contextLoadedCount < _contextTotal;

        public IAsyncOperation<LoadMoreItemsResult> LoadMoreItemsAsync(uint count) =>
            LoadMoreItemsAsyncCore(count).AsAsyncOperation();

        private async Task<LoadMoreItemsResult> LoadMoreItemsAsyncCore(uint count)
        {
            var offset = _contextLoadedCount;
            var (current, resolved) = await _session.RunForCurrentHandle(
                handle => NativeBae.QueueUpcomingPage(handle, checked((uint)offset), count));
            if (!current)
            {
                return new LoadMoreItemsResult { Count = 0 };
            }

            var (page, error) = resolved;
            if (error is not null)
            {
                BaeDiagnostics.Logger.Warning($"Failed to load upcoming queue page at offset {offset}: {error}");
                return new LoadMoreItemsResult { Count = 0 };
            }
            if (page is null || page.Revision != _revision)
            {
                BaeDiagnostics.Logger.Warning(
                    $"Dropping upcoming queue page at offset {offset}: fetched under a since-superseded revision.");
                return new LoadMoreItemsResult { Count = 0 };
            }

            foreach (var entry in page.Entries)
            {
                Add(new EntryRow(QueueLane.Context, entry));
            }
            _contextLoadedCount += (ulong)page.Entries.Length;
            return new LoadMoreItemsResult { Count = (uint)page.Entries.Length };
        }
    }
}
