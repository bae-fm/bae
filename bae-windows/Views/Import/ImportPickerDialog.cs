using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Import-confirm dialog for a candidate: pick an identity (the auto-identified
// matches, or one found by manual search when auto-identification came up empty),
// preview the candidate's audio, choose a storage mode, and import.
internal sealed class ImportPickerDialog
{
    private readonly SessionStore _session;
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly ImportStore _import;
    private readonly PlaybackService _playback;
    private readonly ImportConfirmDialog _confirm;

    public ImportPickerDialog(
        SessionStore session,
        Func<XamlRoot?> xamlRoot,
        ImportStore import,
        PlaybackService playback,
        ImportConfirmDialog confirm)
    {
        _session = session;
        _xamlRoot = xamlRoot;
        _import = import;
        _playback = playback;
        _confirm = confirm;
    }

    public async System.Threading.Tasks.Task Show(ImportCandidate candidate)
    {
        var results = new List<ReleaseCandidateChoice>(candidate.Matches);
        var resultsList = new ListView
        {
            SelectionMode = ListViewSelectionMode.Single,
            MaxHeight = 240,
        };
        void RenderResults() => resultsList.ItemsSource = results.Select(result => result.Summary).ToList();
        RenderResults();

        var artistBox = new TextBox { Header = Loc.Chrome("import.field.artist_manual") };
        var albumBox = new TextBox { Header = Loc.Chrome("search.field.album"), Text = candidate.Name };
        var sourceBox = new ComboBox { Header = Loc.Chrome("search.field.source") };
        sourceBox.Items.Add("discogs");
        sourceBox.Items.Add("musicbrainz");
        sourceBox.SelectedIndex = 0;
        var searchButton = new Button { Content = Loc.Chrome("action.search") };

        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };

        var content = new StackPanel { Spacing = 8, Width = 420 };
        content.Children.Add(new TextBlock { Text = candidate.Name });

        // Preview the candidate's audio before committing to an identity. The live
        // position label follows the import store's preview state while open.
        Action? renderPreview = null;
        if (candidate.AudioPaths.Count > 0)
        {
            var preview = new Button { Content = "▶ " + Loc.Chrome("import.preview") };
            preview.Click += (_, _) => _playback.PreviewPlay(candidate.AudioPaths[0]);
            var pause = new Button { Content = "⏸" };
            pause.Click += (_, _) => _playback.PreviewTogglePause();
            var stop = new Button { Content = "⏹" };
            stop.Click += (_, _) => _playback.PreviewStop();
            // Live preview position, updated by PreviewProgress while the picker is open.
            var previewElapsed = new TextBlock { VerticalAlignment = VerticalAlignment.Center };
            renderPreview = () => previewElapsed.Text = _import.PreviewElapsedText;
            _import.PreviewElapsedChanged += renderPreview;
            var previewRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            previewRow.Children.Add(preview);
            previewRow.Children.Add(pause);
            previewRow.Children.Add(stop);
            previewRow.Children.Add(previewElapsed);
            content.Children.Add(previewRow);
        }

        content.Children.Add(resultsList);
        content.Children.Add(artistBox);
        content.Children.Add(albumBox);
        content.Children.Add(sourceBox);
        content.Children.Add(searchButton);
        content.Children.Add(status);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("import.release_title"),
            Content = new ScrollViewer { Content = content },
            PrimaryButtonText = Loc.Chrome("action.import"),
            SecondaryButtonText = Loc.Chrome("identify.skip"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = _xamlRoot(),
            IsPrimaryButtonEnabled = false,
        };

        // The candidate's readable evidence files (paired CUE sheets, rip logs,
        // info text). Clicking one hides the picker to make room for the viewer
        // (one ContentDialog shows at a time) and hands the decoded text to the
        // loop below; a failed read renders in the picker's status block instead.
        (string Name, string Text)? pendingDocument = null;
        if (candidate.Documents.Count > 0)
        {
            var documentsSection = new StackPanel { Spacing = 8 };
            documentsSection.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("import.documents"),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            });
            foreach (var document in candidate.Documents)
            {
                var docButton = new Button
                {
                    Content = $"{document.Name}  ·  {Loc.Bytes(document.SizeBytes)}",
                    HorizontalAlignment = HorizontalAlignment.Stretch,
                };
                docButton.Click += (_, _) =>
                {
                    var (text, error) = NativeBae.ReadTextFile(document.Path);
                    if (error is not null)
                    {
                        status.Text = Loc.Chrome("import.document.read_failed",
                            new Dictionary<string, object?> { ["name"] = document.Name, ["error"] = error });
                        status.Visibility = Visibility.Visible;
                        return;
                    }
                    pendingDocument = (document.Name, text!);
                    dialog.Hide();
                };
                documentsSection.Children.Add(docButton);
            }
            // Below the candidate name and audio-preview row, above the results.
            content.Children.Insert(renderPreview is null ? 1 : 2, documentsSection);
        }

        searchButton.Click += async (_, _) =>
        {
            var source = (string)sourceBox.SelectedItem;
            var artist = artistBox.Text;
            var album = albumBox.Text;
            searchButton.IsEnabled = false;
            var (current, search) = await _session.RunForCurrentHandle(
                handle => NativeBae.SearchReleases(handle, source, artist, album));
            searchButton.IsEnabled = true;
            if (!current)
            {
                return;
            }
            if (search.Error is not null)
            {
                status.Text = search.Error;
                status.Visibility = Visibility.Visible;
                return;
            }

            results = search.Candidates ?? [];
            RenderResults();
            dialog.IsPrimaryButtonEnabled = false;
        };

        resultsList.SelectionChanged += (_, _) =>
        {
            dialog.IsPrimaryButtonEnabled = resultsList.SelectedIndex >= 0;
        };

        // Clicking Import here doesn't commit — it advances to the metadata edit
        // step. A second ContentDialog can't open while this one shows, so the
        // picker closes on Primary (the selection + storage mode are captured) and
        // the confirm step opens after ShowAsync returns. "Skip identifying"
        // (Secondary) advances the same way but with no picked release — a
        // skip-identify import seeded from the rip's file tags. The loop re-opens
        // this picker — with its search results and selection intact — when the
        // confirm step's "Back to Search" is chosen; Import or Cancel ends the flow.
        try
        {
            while (true)
            {
                var pickerResult = await dialog.ShowAsync();

                // A document click hid the picker to make room for the viewer;
                // present it, then re-open the picker with its search results and
                // selection intact. The audio preview keeps playing underneath, so
                // this path leaves the preview session untouched.
                if (pendingDocument is { } doc)
                {
                    pendingDocument = null;
                    await ShowDocument(doc.Name, doc.Text);
                    continue;
                }

                _session.WithCurrentHandle(NativeBae.PreviewStop);

                ReleaseCandidateChoice? chosen;
                if (pickerResult == ContentDialogResult.Primary)
                {
                    var index = resultsList.SelectedIndex;
                    if (index < 0 || index >= results.Count)
                    {
                        return;
                    }
                    chosen = results[index];
                }
                else if (pickerResult == ContentDialogResult.Secondary)
                {
                    chosen = null;
                }
                else
                {
                    return;
                }

                var backToSearch = await _confirm.Show(candidate, chosen);
                if (!backToSearch)
                {
                    return;
                }
            }
        }
        finally
        {
            if (renderPreview is not null)
            {
                _import.PreviewElapsedChanged -= renderPreview;
            }
            _import.ClearPreview();
        }
    }

    // The document viewer: the file's decoded text, monospace and selectable, in
    // a scrollable modal — the Windows shape of the macOS document overlay. The
    // title is the raw file name; ContentDialog's built-in max width caps the
    // wrapped text.
    private async System.Threading.Tasks.Task ShowDocument(string name, string text)
    {
        var viewer = new ContentDialog
        {
            Title = name,
            Content = new ScrollViewer
            {
                Content = new TextBlock
                {
                    Text = text,
                    FontFamily = new FontFamily("Consolas"),
                    TextWrapping = TextWrapping.Wrap,
                    IsTextSelectionEnabled = true,
                },
                MaxHeight = 480,
            },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = _xamlRoot(),
        };
        await viewer.ShowAsync();
    }
}
