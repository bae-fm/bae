#if DEBUG
using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Graphics;
using Windows.Graphics.Imaging;
using Windows.Storage;
using Windows.Storage.Streams;

namespace Bae.Windows;

// DEBUG-only screenshot capture: `bae-windows.exe --capture-shots <dir>` renders
// each named scene to <dir>\<scene>@windows.png and exits 0 (all scenes rendered)
// or 1 (any scene failed). The scenes reuse the same fixtures (PreviewData) and
// pure builders (WelcomeView, AlbumExpansionRows) the component gallery and the
// production window render — no re-implemented view content. A scene that would
// need a live library handle is absent from the registry, not faked.
//
// The capture runs the real app so RenderTargetBitmap has a live visual tree, but
// App.OnLaunched branches here before any library startup, so it never touches
// the keychain or ~/.bae. Compiled only in DEBUG builds.
internal static class ShotCapture
{
    // The command-line flag App.OnLaunched checks. The output directory is the
    // argument that follows it (the capture script creates it before launch).
    internal const string Flag = "--capture-shots";

    // The platform suffix in the shared "<scene>@<platform>.png" gallery contract.
    private const string Platform = "windows";

    // One capture scene: a stable id (the gallery scene key, shared across
    // platforms) and a fixed logical size, with a builder that renders the
    // composition against fixtures.
    private readonly record struct Scene(string Id, double Width, double Height, Func<FrameworkElement> Build);

    // The scene registry. library-grid is intentionally absent: its album cards
    // are built by MainWindow instance methods bound to a live grid layout and
    // library handle (cover images), not a pure builder, so it cannot be staged
    // honestly from fixtures — omitted rather than faked.
    private static IReadOnlyList<Scene> Scenes { get; } = new[]
    {
        new Scene("welcome", 900, 600, BuildWelcome),
        new Scene("album-detail", 720, 540, BuildAlbumDetail),
    };

    // True when args carry the capture flag; then outputDir is the directory that
    // follows it. Throws when the flag is present without a directory — a
    // malformed capture request fails loudly rather than falling through to the
    // real app (which would open the keychain).
    internal static bool TryGetOutputDir(IReadOnlyList<string> args, out string outputDir)
    {
        for (var i = 0; i < args.Count; i++)
        {
            if (args[i] != Flag)
            {
                continue;
            }
            if (i + 1 >= args.Count || string.IsNullOrEmpty(args[i + 1]))
            {
                throw new InvalidOperationException($"{Flag} requires an output directory argument.");
            }
            outputDir = args[i + 1];
            return true;
        }

        outputDir = string.Empty;
        return false;
    }

    // Render every scene to <outputDir>\<id>@windows.png, then exit the process.
    // Each scene is attempted; a scene that throws is reported and marks the run
    // failed, but the rest still render, so one broken scene never hides the
    // others. The exit code is 0 only when every scene succeeded.
    internal static async Task RunAsync(string outputDir)
    {
        var failed = false;
        try
        {
            var folder = await StorageFolder.GetFolderFromPathAsync(outputDir);
            foreach (var scene in Scenes)
            {
                try
                {
                    await CaptureAsync(scene, folder);
                }
                catch (Exception exception)
                {
                    failed = true;
                    Console.Error.WriteLine($"capture-shots: scene '{scene.Id}' failed: {exception}");
                }
            }
        }
        catch (Exception exception)
        {
            failed = true;
            Console.Error.WriteLine($"capture-shots: fatal: {exception}");
        }

        Environment.Exit(failed ? 1 : 0);
    }

    private static async Task CaptureAsync(Scene scene, StorageFolder folder)
    {
        var root = new Grid
        {
            Width = scene.Width,
            Height = scene.Height,
            // A solid page background so the PNG is opaque rather than the
            // window's Mica; the app runs dark-themed (set in App's constructor),
            // so this resolves to the dark background.
            Background = (Brush)Application.Current.Resources["ApplicationPageBackgroundThemeBrush"],
        };
        root.Children.Add(scene.Build());

        var window = new Window();
        var loaded = new TaskCompletionSource();
        root.Loaded += (_, _) => loaded.TrySetResult();
        window.Content = root;
        window.Activate();

        // AppWindow speaks physical pixels; size the client so the whole scene is
        // realized. RenderTargetBitmap captures only realized, on-screen content,
        // so a client smaller than the scene would render the overflow blank.
        var scale = root.XamlRoot?.RasterizationScale ?? 1.0;
        window.AppWindow.ResizeClient(new SizeInt32(
            (int)Math.Ceiling(scene.Width * scale), (int)Math.Ceiling(scene.Height * scale)));

        await loaded.Task;
        root.UpdateLayout();
        // Let the compositor commit one frame before reading pixels back.
        await WaitForNextFrameAsync();

        var bitmap = new RenderTargetBitmap();
        await bitmap.RenderAsync(root);

        var pixelBuffer = await bitmap.GetPixelsAsync();
        var pixels = new byte[pixelBuffer.Length];
        using (var reader = DataReader.FromBuffer(pixelBuffer))
        {
            reader.ReadBytes(pixels);
        }

        var file = await folder.CreateFileAsync(
            $"{scene.Id}@{Platform}.png", CreationCollisionOption.ReplaceExisting);
        using (var stream = await file.OpenAsync(FileAccessMode.ReadWrite))
        {
            var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, stream);
            encoder.SetPixelData(
                BitmapPixelFormat.Bgra8,
                BitmapAlphaMode.Premultiplied,
                (uint)bitmap.PixelWidth,
                (uint)bitmap.PixelHeight,
                96,
                96,
                pixels);
            await encoder.FlushAsync();
        }

        window.Close();
    }

    // Complete once the compositor renders its next frame, so the scene's layout
    // is committed before RenderTargetBitmap reads it back.
    private static Task WaitForNextFrameAsync()
    {
        var tcs = new TaskCompletionSource();
        void OnRendering(object? sender, object args)
        {
            CompositionTarget.Rendering -= OnRendering;
            tcs.TrySetResult();
        }

        CompositionTarget.Rendering += OnRendering;
        return tcs.Task;
    }

    // The first-run welcome chooser, staged with fixture libraries and no-op
    // callbacks. Drives WelcomeView's real Show() — the production path — rather
    // than a re-implementation of it.
    private static FrameworkElement BuildWelcome()
    {
        var host = new StackPanel
        {
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        var welcome = new WelcomeView(
            host,
            setStatus: _ => { },
            loadLibraries: () => PreviewData.WelcomeLibraries,
            createLibrary: _ => null,
            openLibrary: _ => { },
            showJoinLibrary: () => Task.CompletedTask,
            showRestoreFromCloud: () => Task.CompletedTask);
        welcome.Show();
        return host;
    }

    // The album detail composition: the header block and track rows the
    // production expansion (AlbumExpansionPanel) and the component gallery build
    // from the shared AlbumExpansionRows builders, here over PreviewData fixtures.
    private static FrameworkElement BuildAlbumDetail()
    {
        var panel = new StackPanel
        {
            Spacing = 12,
            Padding = new Thickness(24),
            VerticalAlignment = VerticalAlignment.Center,
        };
        panel.Children.Add(AlbumExpansionRows.BuildHeaderBlock(
            PreviewData.ExpansionTitle, PreviewData.ExpansionArtist));
        foreach (var track in PreviewData.ExpansionTracks)
        {
            panel.Children.Add(AlbumExpansionRows.BuildTrackRow(
                track.Position,
                track.Title,
                track.Artist,
                track.Duration,
                onPlay: () => { },
                onPlayNext: () => { },
                onAddToQueue: () => { },
                onExportTrack: () => { }));
        }

        return panel;
    }
}
#endif
