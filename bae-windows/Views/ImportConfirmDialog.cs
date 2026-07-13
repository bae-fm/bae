using System;
using System.Collections.Generic;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The import confirmation step: seed the album/pressing/track edit form from the
// chosen candidate release, let the user revise it, then commit the import with
// those edits overlaid. Errors stay in-dialog (the window banner is occluded by
// the modal). Mirrors macOS's ImportConfirmationView.
internal sealed class ImportConfirmDialog
{
    private readonly SessionStore _session;
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Func<string, System.Threading.Tasks.Task> _openAlbum;
    private readonly LightboxOverlay _lightbox;

    public ImportConfirmDialog(
        SessionStore session,
        Func<XamlRoot?> xamlRoot,
        Func<string, System.Threading.Tasks.Task> openAlbum,
        LightboxOverlay lightbox)
    {
        _session = session;
        _xamlRoot = xamlRoot;
        _openAlbum = openAlbum;
        _lightbox = lightbox;
    }

    // Returns true when the user chose "Back to Search" — the caller re-opens the
    // picker so they can pick or search for a different release. A null chosen is
    // the skip-identify import: no source release, seeded from the rip's file tags.
    public async System.Threading.Tasks.Task<bool> Show(
        ImportCandidate candidate, ReleaseCandidateChoice? chosen)
    {
        var (current, seed) = await _session.RunForCurrentHandle(
            handle => chosen is null
                ? NativeBae.PrefetchUnknownEdit(handle, candidate.FolderPath)
                : NativeBae.PrefetchCandidateEdit(handle, chosen.ReleaseId, chosen.Source, candidate.FolderPath));
        if (!current)
        {
            return false;
        }
        if (seed.Prefetched is null)
        {
            await DialogPrimitives.ShowError(_xamlRoot(), Loc.Chrome("import.error.load_release"), seed.Error);
            return false;
        }
        var prefetched = seed.Prefetched;

        var (settingsCurrent, settings) = await _session.RunForCurrentHandle(NativeBae.GetSettings);
        if (!settingsCurrent)
        {
            return false;
        }
        var form = new ReleaseEditForm(prefetched.Edit, 520);

        // No selection means "let the import choose its default cover" (the
        // source's first cover art, else a folder image).
        BridgeCoverSelection? selectedCover = null;
        var storageRemote = new CheckBox
        {
            Content = Loc.Chrome("import.storage.managed"),
            IsChecked = true,
            VerticalAlignment = VerticalAlignment.Center,
        };
        var storagePinned = new CheckBox
        {
            Content = Loc.Chrome("import.storage.keep_local"),
            IsChecked = true,
            VerticalAlignment = VerticalAlignment.Center,
        };
        bool StorageRemoteSelected() => settings.HasCloudHome && storageRemote.IsChecked == true;
        // Storage state and pinned-ness are orthogonal: the mode tag is purely
        // the remote-vs-local storage choice; the pin choice rides alongside as
        // its own arg, meaningful only for a remote import.
        string StorageModeTag() => StorageRemoteSelected() ? "managed" : "unmanaged";
        bool StoragePinSelected() => StorageRemoteSelected() && storagePinned.IsChecked == true;

        void RefreshStorageControls()
        {
            storagePinned.Visibility = StorageRemoteSelected()
                ? Visibility.Visible
                : Visibility.Collapsed;
        }
        storageRemote.Checked += (_, _) => RefreshStorageControls();
        storageRemote.Unchecked += (_, _) => RefreshStorageControls();
        RefreshStorageControls();

        var panel = new StackPanel { Spacing = 8, MinWidth = 520 };
        if (settings.HasCloudHome)
        {
            var storageRow = new StackPanel
            {
                Orientation = Orientation.Horizontal,
                Spacing = 8,
                HorizontalAlignment = HorizontalAlignment.Right,
                VerticalAlignment = VerticalAlignment.Center,
            };
            storageRow.Children.Add(storageRemote);
            storageRow.Children.Add(storagePinned);
            panel.Children.Add(storageRow);
        }
        panel.Children.Add(BuildCoverPicker(
            prefetched.RemoteCovers, prefetched.LocalArtwork, picked => selectedCover = picked));

        // A source-backed pick claims the exact pressing by default, or metadata
        // only (just the album group). Flipping re-shapes the form from the kept
        // editor seed with no re-fetch, overwriting in-progress edits, and
        // disables the pressing fields when metadata-only so the core-side blanking
        // is visible. A skip-identify import has no source release and no choice.
        var identity = new ImportIdentityModel(chosen is not null);
        if (identity.ShowsExactnessChoice)
        {
            var note = new TextBlock
            {
                Text = Loc.Chrome("identify.metadata_only_note"),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                TextWrapping = TextWrapping.Wrap,
                Visibility = identity.ShowsMetadataOnlyNote ? Visibility.Visible : Visibility.Collapsed,
            };

            void ApplyClaim(bool metadataOnly)
            {
                identity.SetMetadataOnly(metadataOnly);
                note.Visibility = identity.ShowsMetadataOnlyNote ? Visibility.Visible : Visibility.Collapsed;
                form.Seed(NativeBae.ShapeCandidateEdit(
                    prefetched.Seed!,
                    NativeBae.SourceIdentityChoice(!identity.MetadataOnly, chosen!.ReleaseId, chosen.Source)));
                form.SetPressingFieldsEnabled(identity.PressingFieldsEnabled);
            }

            var exactRadio = new RadioButton
            {
                Content = Loc.Chrome("identify.exact_pressing"),
                GroupName = "importIdentityClaim",
                IsChecked = true,
            };
            var metadataRadio = new RadioButton
            {
                Content = Loc.Chrome("identify.metadata_only"),
                GroupName = "importIdentityClaim",
            };
            exactRadio.Checked += (_, _) => ApplyClaim(false);
            metadataRadio.Checked += (_, _) => ApplyClaim(true);

            var claimRow = new StackPanel
            {
                Orientation = Orientation.Horizontal,
                Spacing = 8,
                VerticalAlignment = VerticalAlignment.Center,
            };
            claimRow.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("identify.import_as"),
                VerticalAlignment = VerticalAlignment.Center,
            });
            claimRow.Children.Add(exactRadio);
            claimRow.Children.Add(metadataRadio);

            panel.Children.Add(claimRow);
            panel.Children.Add(note);
        }

        panel.Children.Add(form.Panel);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("import.confirm_title"),
            Content = new ScrollViewer { Content = panel },
            PrimaryButtonText = Loc.Chrome("action.import"),
            SecondaryButtonText = Loc.Chrome("import.back_to_search"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = _xamlRoot(),
        };

        // Whether the user clicked "view in library" on the already-in-library
        // banner. A nested ContentDialog can't open over this one, so the banner
        // closes the confirm dialog and the album opens after ShowAsync returns
        // (the gallery/edit/re-identify pattern).
        string? viewInLibraryAlbumId = null;

        // Tell the user when the chosen release is already in the library before
        // they import a duplicate. The check is by release identity (the confirm
        // flow has no group id), so it reports the exact pressing — album_in_library
        // tracks release_in_library here. Reads the database — run it off the UI
        // thread. A failure leaves the banner absent; the import still proceeds
        // (the banner is advisory, not a gate). A skip-identify import has no
        // release to check, so there is nothing to warn about.
        if (chosen is not null)
        {
            var (statusCurrent, libraryStatus) = await _session.RunForCurrentHandle(
                handle => NativeBae.CheckReleaseInLibrary(handle, chosen.ReleaseId).Status);
            if (!statusCurrent)
            {
                return false;
            }
            if (libraryStatus is not null && libraryStatus.ReleaseInLibrary)
            {
                panel.Children.Insert(0, BuildLibraryStatusBanner(libraryStatus, () =>
                {
                    viewInLibraryAlbumId = libraryStatus.AlbumId;
                    dialog.Hide();
                }));
            }
        }

        // The import runs in the background; its result updates the candidate row
        // and library grid through invalidations.
        // Shape (validation) of the edit happens in Rust — on failure keep the
        // dialog open and show the reason.
        dialog.PrimaryButtonClick += async (_, args) =>
        {
            var deferral = args.GetDeferral();
            var payload = form.ReadBack();
            var storageMode = StorageModeTag();
            var pin = StoragePinSelected();
            BridgeIdentityChoice claim = chosen is null
                ? new BridgeIdentityChoice.Unknown()
                : NativeBae.SourceIdentityChoice(!identity.MetadataOnly, chosen.ReleaseId, chosen.Source);
            var (importCurrent, error) = await _session.RunForCurrentHandle(
                handle => NativeBae.ImportCandidate(
                    handle, candidate.Key, candidate.FolderPath, claim, storageMode, pin, payload, selectedCover));
            if (!importCurrent)
            {
                args.Cancel = true;
                deferral.Complete();
                return;
            }
            if (error is not null)
            {
                form.ErrorText.Text = error;
                form.ErrorText.Visibility = Visibility.Visible;
                args.Cancel = true;
            }

            deferral.Complete();
        };

        var result = await dialog.ShowAsync();

        // "view in library" closed the dialog above; open that album now that the
        // confirm dialog is gone (a nested ContentDialog can't open over it).
        if (viewInLibraryAlbumId is not null)
        {
            await _openAlbum(viewInLibraryAlbumId);
            return false;
        }

        // "Back to Search" (Secondary) returns the user to the picker; Import and
        // Cancel both end the flow.
        return result == ContentDialogResult.Secondary;
    }

    // The already-in-library banner shown at the top of the import confirmation
    // when the chosen release is already in the library. When an album id is
    // present it offers a "view in library" button that invokes onViewInLibrary.
    private static InfoBar BuildLibraryStatusBanner(BridgeLibraryStatus status, Action onViewInLibrary)
    {
        var banner = new InfoBar
        {
            Severity = InfoBarSeverity.Warning,
            IsOpen = true,
            IsClosable = false,
            Message = Loc.Chrome("import.already_in_library"),
        };

        if (!string.IsNullOrEmpty(status.AlbumId))
        {
            var viewButton = new Button { Content = Loc.Chrome("import.view_in_library") };
            viewButton.Click += (_, _) => onViewInLibrary();
            banner.ActionButton = viewButton;
        }

        return banner;
    }

    // The import confirmation's cover-art picker: a gallery of the chosen release's
    // remote covers (thumbnails by URL) and the candidate folder's local artwork
    // (thumbnails by disk path). Clicking a tile selects it, highlights it, and
    // reports its cover selection via onPick; double-tapping local artwork opens
    // the lightbox. Nothing is picked by default, so the import uses its own default
    // cover. Renders inline (not a nested dialog, which can't open over the confirm
    // dialog).
    private StackPanel BuildCoverPicker(
        List<BridgeRemoteCover> remoteCovers, List<LocalArtwork> localArtwork, Action<BridgeCoverSelection> onPick)
    {
        var section = new StackPanel { Spacing = 4 };
        section.Children.Add(new TextBlock { Text = Loc.Chrome("cover.section_title") });

        if (remoteCovers.Count == 0 && localArtwork.Count == 0)
        {
            section.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("cover.none_available"),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                TextWrapping = TextWrapping.Wrap,
            });
            return section;
        }

        section.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("cover.pick_hint"),
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            TextWrapping = TextWrapping.Wrap,
        });

        var grid = new VariableSizedWrapGrid
        {
            Orientation = Orientation.Horizontal,
            ItemWidth = 140,
            ItemHeight = 160,
        };

        // Highlight only the picked tile: clear every tile's border, then mark
        // the clicked one. Each tile carries its own cover selection.
        var tiles = new List<Button>();
        void Select(Button picked, BridgeCoverSelection selection)
        {
            foreach (var tile in tiles)
            {
                tile.BorderThickness = new Thickness(0);
            }

            picked.BorderThickness = new Thickness(2);
            picked.BorderBrush = new SolidColorBrush(Microsoft.UI.Colors.DeepSkyBlue);
            onPick(selection);
        }

        void AddTile(ImageSource? source, string caption, BridgeCoverSelection selection, bool isLocal = false, int localIndex = 0)
        {
            var tile = DialogPrimitives.CoverTile(source, caption);
            tile.Click += (_, _) => Select(tile, selection);
            if (isLocal)
            {
                tile.DoubleTapped += (_, _) => OpenLocalArtworkLightbox(localArtwork, localIndex);
            }
            tiles.Add(tile);
            grid.Children.Add(tile);
        }

        // A malformed cover URL or artwork path would throw synchronously out of
        // this static builder (and the async-void caller) — skip that tile instead
        // of crashing the picker, matching the change-cover gallery's guard.
        foreach (var cover in remoteCovers)
        {
            BitmapImage source;
            try
            {
                source = new BitmapImage(new Uri(NativeBae.RemoteCoverThumbnailUrl(cover)));
            }
            catch (UriFormatException)
            {
                continue;
            }

            var selection = NativeBae.RemoteCoverSelection(cover);
            AddTile(source, cover.Label, selection, isLocal: false);
        }

        var localArtworkCount = 0;
        foreach (var art in localArtwork)
        {
            BitmapImage source;
            try
            {
                source = new BitmapImage(new Uri(art.Path));
            }
            catch (UriFormatException)
            {
                continue;
            }

            var selection = new BridgeCoverSelection.ReleaseImage(art.FileId);
            var currentIndex = localArtworkCount;
            AddTile(source, System.IO.Path.GetFileName(art.FileId), selection, isLocal: true, localIndex: currentIndex);
            localArtworkCount++;
        }

        section.Children.Add(grid);
        return section;
    }

    private void OpenLocalArtworkLightbox(List<LocalArtwork> localArtwork, int startIndex)
    {
        var entries = new List<LightboxEntry>();
        foreach (var art in localArtwork)
        {
            var capturedPath = art.Path;
            entries.Add(new LightboxEntry(
                art.FileId,
                System.IO.Path.GetFileName(art.Path),
                () =>
                {
                    try
                    {
                        return System.IO.File.ReadAllBytes(capturedPath);
                    }
                    catch (Exception)
                    {
                        return null;
                    }
                }));
        }

        _lightbox.Show(entries, startIndex);
    }
}
