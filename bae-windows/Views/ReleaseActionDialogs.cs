using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The per-release actions reachable from album detail: export a release, delete
// it, edit its metadata, re-identify it, browse its gallery, and change its
// cover. Each opens its own dialog (a nested ContentDialog can't open over album
// detail, so album detail closes first and calls into here). Errors that would be
// occluded by a modal surface inside the dialog; the rest go to the status line.
internal sealed class ReleaseActionDialogs
{
    private readonly SessionStore _session;
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Func<IntPtr> _windowHandle;
    private readonly Action<string> _setStatus;

    public ReleaseActionDialogs(
        SessionStore session,
        Func<XamlRoot?> xamlRoot,
        Func<IntPtr> windowHandle,
        Action<string> setStatus)
    {
        _session = session;
        _xamlRoot = xamlRoot;
        _windowHandle = windowHandle;
        _setStatus = setStatus;
    }

    public async System.Threading.Tasks.Task ShowExportRelease(string releaseId)
    {
        var (settingsCurrent, settings) = await _session.RunForCurrentHandle(NativeBae.GetSettings);
        if (!settingsCurrent)
        {
            return;
        }
        var choices = new List<(string Label, BridgeExportSelection Selection)>
        {
            (Loc.Chrome("track.export.original"), ExportSelection.Original()),
        };
        choices.AddRange(settings.ExportPresets
            .Where(preset => preset.AppliesToRelease)
            .Select(preset => (preset.Name, ExportSelection.Preset(preset.Id))));

        var formatPicker = new ComboBox
        {
            Header = Loc.Chrome("settings.export.default_release_format"),
            MinWidth = 260,
        };
        var defaultIndex = -1;
        for (var index = 0; index < choices.Count; index++)
        {
            var choice = choices[index];
            if (ExportSelection.Equal(choice.Selection, settings.DefaultReleaseExportSelection))
            {
                defaultIndex = index;
            }
            formatPicker.Items.Add(new ComboBoxItem
            {
                Content = choice.Label,
                Tag = choice.Selection,
            });
        }
        if (defaultIndex < 0)
        {
            _setStatus(Loc.Chrome("track.export.prepare_failed"));
            return;
        }
        formatPicker.SelectedIndex = defaultIndex;

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("settings.export.label"),
            Content = formatPicker,
            PrimaryButtonText = Loc.Chrome("settings.export.label"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = _xamlRoot(),
        };
        var result = await dialog.ShowAsync();
        if (result != ContentDialogResult.Primary)
        {
            return;
        }
        if (formatPicker.SelectedItem is not ComboBoxItem selectedItem
            || selectedItem.Tag is not BridgeExportSelection selection)
        {
            _setStatus(Loc.Chrome("track.export.prepare_failed"));
            return;
        }

        var picker = new global::Windows.Storage.Pickers.FolderPicker();
        picker.FileTypeFilter.Add("*");
        WinRT.Interop.InitializeWithWindow.Initialize(picker, _windowHandle());
        var folder = await picker.PickSingleFolderAsync();
        if (folder is null)
        {
            return;
        }

        var (exportCurrent, error) = await _session.RunForCurrentHandle(
            handle => NativeBae.ExportRelease(handle, releaseId, folder.Path, selection));
        if (!exportCurrent)
        {
            return;
        }
        if (error is not null)
        {
            _setStatus(error);
        }
    }

    public async System.Threading.Tasks.Task ConfirmDeleteRelease(string releaseId)
    {
        if (string.IsNullOrEmpty(releaseId))
        {
            return;
        }

        var confirm = new ContentDialog
        {
            Title = Loc.Chrome("album.delete.title"),
            Content = Loc.Chrome("album.delete.body"),
            PrimaryButtonText = Loc.Chrome("action.delete"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = _xamlRoot(),
        };
        if (await confirm.ShowAsync() != ContentDialogResult.Primary)
        {
            return;
        }

        // On success an invalidation refreshes the grid.
        var (current, error) = await _session.RunForCurrentHandle(
            handle => NativeBae.DeleteRelease(handle, releaseId));
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            await DialogPrimitives.ShowError(_xamlRoot(), Loc.Chrome("album.delete.failed"), error);
        }
    }

    public async System.Threading.Tasks.Task ShowEditMetadata(string releaseId)
    {
        if (string.IsNullOrEmpty(releaseId))
        {
            return;
        }

        var (current, seeded) = _session.WithCurrentHandle(
            handle => NativeBae.ReleaseEditSeed(handle, releaseId).Edit);
        if (!current)
        {
            return;
        }
        if (seeded is null)
        {
            await DialogPrimitives.ShowError(_xamlRoot(), Loc.Chrome("album.edit.load_failed"));
            return;
        }

        var form = new ReleaseEditForm(seeded, 520);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("album.edit.title"),
            Content = new ScrollViewer { Content = form.Panel },
            PrimaryButtonText = Loc.Chrome("action.save"),
            SecondaryButtonText = Loc.Chrome("album.edit.reset"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = _xamlRoot(),
        };

        // Shape + write happen in Rust; on a validation/write error keep the
        // dialog open and show the reason instead of committing.
        dialog.PrimaryButtonClick += (_, args) =>
        {
            var (editCurrent, error) = _session.WithCurrentHandle(
                handle => NativeBae.ApplyReleaseEdit(handle, releaseId, form.ReadBack()));
            if (!editCurrent)
            {
                args.Cancel = true;
                return;
            }
            if (error is not null)
            {
                form.ErrorText.Text = error;
                form.ErrorText.Visibility = Visibility.Visible;
                args.Cancel = true;
            }
        };

        // Reset to Source discards the in-progress edits and re-seeds the form
        // from the release's stored metadata source (its original identity)
        // without writing the DB. Keep the dialog open regardless; a deferral
        // holds it through the async re-projection so it can't close mid-await.
        dialog.SecondaryButtonClick += async (_, args) =>
        {
            args.Cancel = true;
            var deferral = args.GetDeferral();
            try
            {
                var (resetCurrent, fresh) = await _session.RunForCurrentHandle(
                    handle => NativeBae.ResetMetadataToSource(handle, releaseId).Edit);
                if (!resetCurrent)
                {
                    return;
                }
                if (fresh is null)
                {
                    form.ErrorText.Text = Loc.Chrome("album.edit.reset_failed");
                    form.ErrorText.Visibility = Visibility.Visible;
                    return;
                }

                form.ErrorText.Visibility = Visibility.Collapsed;
                form.Seed(fresh);
            }
            finally
            {
                deferral.Complete();
            }
        };

        await dialog.ShowAsync();
    }

    public async System.Threading.Tasks.Task ShowReidentify(string releaseId, string seedArtist, string seedAlbum)
    {
        if (string.IsNullOrEmpty(releaseId))
        {
            return;
        }

        var artistBox = new TextBox { Header = Loc.Chrome("search.field.artist"), Text = seedArtist };
        var albumBox = new TextBox { Header = Loc.Chrome("search.field.album"), Text = seedAlbum };
        var sourceBox = new ComboBox { Header = Loc.Chrome("search.field.source") };
        sourceBox.Items.Add("discogs");
        sourceBox.Items.Add("musicbrainz");
        sourceBox.SelectedIndex = 0;
        var searchButton = new Button { Content = Loc.Chrome("action.search") };

        var resultsList = new ListView
        {
            SelectionMode = ListViewSelectionMode.Single,
            MaxHeight = 280,
        };
        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };

        // The identity claim for the selected pressing: exact by default, or
        // metadata only. Hidden until a result is picked, and reset to exact on
        // every new pick. A skip-identify re-identify (the Secondary button) claims
        // nothing here — core reseeds the rows from the rip's file tags.
        var exactRadio = new RadioButton
        {
            Content = Loc.Chrome("identify.exact_pressing"),
            GroupName = "reidentifyIdentityClaim",
            IsChecked = true,
        };
        var metadataRadio = new RadioButton
        {
            Content = Loc.Chrome("identify.metadata_only"),
            GroupName = "reidentifyIdentityClaim",
        };
        var claimRow = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            VerticalAlignment = VerticalAlignment.Center,
            Visibility = Visibility.Collapsed,
        };
        claimRow.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("album.reidentify.claim_header"),
            VerticalAlignment = VerticalAlignment.Center,
        });
        claimRow.Children.Add(exactRadio);
        claimRow.Children.Add(metadataRadio);

        var identity = new ImportIdentityModel(hasSourceRelease: true);
        exactRadio.Checked += (_, _) => identity.SetMetadataOnly(false);
        metadataRadio.Checked += (_, _) => identity.SetMetadataOnly(true);

        var form = new StackPanel { Spacing = 8, Width = 420 };
        form.Children.Add(artistBox);
        form.Children.Add(albumBox);
        form.Children.Add(sourceBox);
        form.Children.Add(searchButton);
        form.Children.Add(resultsList);
        form.Children.Add(claimRow);
        form.Children.Add(status);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("album.reidentify.title"),
            Content = new ScrollViewer { Content = form },
            PrimaryButtonText = Loc.Chrome("album.reidentify.confirm"),
            SecondaryButtonText = Loc.Chrome("identify.skip"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = _xamlRoot(),
            IsPrimaryButtonEnabled = false,
        };

        var candidates = new List<ReleaseCandidateChoice>();

        // The generated bridge search and commit both block on network/DB work; run them off
        // the UI thread so the dialog stays responsive.
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

            candidates = search.Candidates ?? [];
            resultsList.ItemsSource = candidates.Select(candidate => candidate.Summary).ToList();
            status.Text = Loc.Chrome("search.no_matches");
            status.Visibility = candidates.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
            dialog.IsPrimaryButtonEnabled = false;
        };

        resultsList.SelectionChanged += (_, _) =>
        {
            var selected = resultsList.SelectedIndex >= 0;
            dialog.IsPrimaryButtonEnabled = selected;
            claimRow.Visibility = selected ? Visibility.Visible : Visibility.Collapsed;
            // Each new pick resets the claim to exact.
            identity = new ImportIdentityModel(hasSourceRelease: true);
            exactRadio.IsChecked = true;
        };

        dialog.PrimaryButtonClick += async (_, args) =>
        {
            var index = resultsList.SelectedIndex;
            if (index < 0 || index >= candidates.Count)
            {
                args.Cancel = true;
                return;
            }

            var chosen = candidates[index];
            var deferral = args.GetDeferral();
            var (current, error) = await _session.RunForCurrentHandle(
                handle => NativeBae.ReidentifyRelease(handle, releaseId,
                    NativeBae.SourceIdentityChoice(!identity.MetadataOnly, chosen.ReleaseId, chosen.Source)));
            if (!current)
            {
                args.Cancel = true;
                deferral.Complete();
                return;
            }
            if (error is not null)
            {
                status.Text = error;
                status.Visibility = Visibility.Visible;
                args.Cancel = true;
            }

            deferral.Complete();
        };

        // "Skip identifying" clears any source identity in one click: core reseeds
        // the release's rows from the rip's file tags, so there is no editable seed
        // page and no follow-up refresh prompt.
        dialog.SecondaryButtonClick += async (_, args) =>
        {
            var deferral = args.GetDeferral();
            var (current, error) = await _session.RunForCurrentHandle(
                handle => NativeBae.ReidentifyRelease(handle, releaseId, new BridgeIdentityChoice.Unknown()));
            if (!current)
            {
                args.Cancel = true;
                deferral.Complete();
                return;
            }
            if (error is not null)
            {
                status.Text = error;
                status.Visibility = Visibility.Visible;
                args.Cancel = true;
            }

            deferral.Complete();
        };

        await dialog.ShowAsync();
    }

    public async System.Threading.Tasks.Task ShowGallery(string releaseId)
    {
        var (current, images) = _session.WithCurrentHandle(
            handle => NativeBae.Gallery(handle, releaseId).Items);
        if (!current)
        {
            return;
        }
        if (images is null)
        {
            _setStatus(Loc.Chrome("gallery.load_failed"));
            return;
        }

        if (images.Length == 0)
        {
            return;
        }

        var index = 0;
        var image = new Image { Stretch = Stretch.Uniform, MinHeight = 360, MinWidth = 360 };
        var label = new TextBlock { HorizontalAlignment = HorizontalAlignment.Center };
        void Show()
        {
            var item = images[index];
            var handle = _session.CurrentHandleOrNull();
            if (handle == null)
            {
                return;
            }
            image.Source = CoverImage.LoadGalleryBytes(handle, releaseId, item.Source);
            label.Text = $"{item.Label} ({index + 1}/{images.Length})";
        }
        Show();

        var prev = new Button { Content = "‹" };
        var next = new Button { Content = "›" };
        prev.Click += (_, _) => { index = (index - 1 + images.Length) % images.Length; Show(); };
        next.Click += (_, _) => { index = (index + 1) % images.Length; Show(); };

        var nav = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            HorizontalAlignment = HorizontalAlignment.Center,
            Spacing = 12,
        };
        nav.Children.Add(prev);
        nav.Children.Add(label);
        nav.Children.Add(next);

        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(image);
        content.Children.Add(nav);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("gallery.title"),
            Content = content,
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = _xamlRoot(),
        };
        await dialog.ShowAsync();
    }

    // Pick a new cover for the release: its own image files plus remote candidates
    // fetched from MusicBrainz / Discogs. Selecting one writes it as the release's
    // cover; the album grid refreshes via the invalidation the change emits. Errors
    // surface inside this dialog, since the window banner is occluded by the modal.
    public async System.Threading.Tasks.Task ShowChangeCover(string albumId, string releaseId)
    {
        if (string.IsNullOrEmpty(albumId) || string.IsNullOrEmpty(releaseId))
        {
            return;
        }

        var (imagesCurrent, releaseImages) = _session.WithCurrentHandle(
            handle => NativeBae.GetReleaseImages(handle, releaseId).Images);
        if (!imagesCurrent)
        {
            return;
        }
        if (releaseImages is null)
        {
            _setStatus(Loc.Chrome("cover.images_load_failed"));
            return;
        }

        var content = new StackPanel { Spacing = 8, MinWidth = 460 };

        // Errors from a failed remote fetch or a failed change surface here; the
        // window-level banner is hidden behind this modal dialog.
        var statusText = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            Visibility = Visibility.Collapsed,
        };
        content.Children.Add(statusText);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("cover.change_title"),
            Content = new ScrollViewer { Content = content, MaxHeight = 520 },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = _xamlRoot(),
        };

        // Apply a selection off the UI thread (a remote cover downloads bytes),
        // then close on success or show the error in place.
        async System.Threading.Tasks.Task Apply(BridgeCoverSelection selection)
        {
            statusText.Visibility = Visibility.Collapsed;
            var (current, error) = await _session.RunForCurrentHandle(
                handle => NativeBae.ChangeCover(handle, albumId, releaseId, selection));
            if (!current)
            {
                return;
            }
            if (error is null)
            {
                dialog.Hide();
            }
            else
            {
                statusText.Text = error;
                statusText.Visibility = Visibility.Visible;
            }
        }

        // A thumbnail tile that applies the selection when clicked.
        Button Tile(ImageSource? source, string caption, BridgeCoverSelection selection)
        {
            var button = DialogPrimitives.CoverTile(source, caption);
            button.Click += async (_, _) => await Apply(selection);
            return button;
        }

        if (releaseImages.Length > 0)
        {
            content.Children.Add(new TextBlock { Text = Loc.Chrome("cover.release_files") });
            var fileGrid = new VariableSizedWrapGrid
            {
                Orientation = Orientation.Horizontal,
                ItemWidth = 140,
                ItemHeight = 160,
            };
            foreach (var file in releaseImages)
            {
                var handle = _session.CurrentHandleOrNull();
                if (handle == null)
                {
                    return;
                }
                var source = CoverImage.LoadGalleryBytes(
                    handle, releaseId, new BridgeGallerySource.ReleaseFile(file.Id));
                var selection = new BridgeCoverSelection.ReleaseImage(file.Id);
                fileGrid.Children.Add(Tile(source, file.OriginalFilename, selection));
            }

            content.Children.Add(fileGrid);
        }

        content.Children.Add(new TextBlock { Text = Loc.Chrome("cover.remote_sources") });
        var loading = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
        };
        loading.Children.Add(new ProgressRing { IsActive = true, Width = 20, Height = 20 });
        loading.Children.Add(new TextBlock { Text = Loc.Chrome("cover.fetching") });
        content.Children.Add(loading);

        var remoteGrid = new VariableSizedWrapGrid
        {
            Orientation = Orientation.Horizontal,
            ItemWidth = 140,
            ItemHeight = 160,
        };
        content.Children.Add(remoteGrid);

        // Fetch the remote candidates off the UI thread, then fill the grid on
        // resume. The dialog opens immediately with the release files shown and a
        // spinner where the remote covers will land.
        async System.Threading.Tasks.Task LoadRemote()
        {
            var (current, covers) = await _session.RunForCurrentHandle(
                handle => NativeBae.FetchRemoteCovers(handle, releaseId).Covers);
            if (!current)
            {
                return;
            }
            loading.Visibility = Visibility.Collapsed;
            if (covers is null)
            {
                statusText.Text = Loc.Chrome("cover.fetch_failed");
                statusText.Visibility = Visibility.Visible;
                return;
            }

            try
            {
                if (covers.Length == 0)
                {
                    remoteGrid.Children.Add(new TextBlock { Text = Loc.Chrome("cover.none_remote") });
                    return;
                }

                foreach (var cover in covers)
                {
                    var source = new BitmapImage(new Uri(NativeBae.RemoteCoverThumbnailUrl(cover)));
                    var selection = NativeBae.RemoteCoverSelection(cover);
                    remoteGrid.Children.Add(Tile(source, cover.Label, selection));
                }
            }
            catch (Exception ex)
            {
                // Fire-and-forget: a malformed cover URL or unexpected payload must
                // surface here, not as an unobserved task exception.
                statusText.Text = Loc.Chrome("cover.show_failed", "detail", ex.Message);
                statusText.Visibility = Visibility.Visible;
            }
        }

        _ = LoadRemote();
        await dialog.ShowAsync();
    }
}
