#if DEBUG
using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Microsoft.UI.Dispatching;
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
// scenes to <dir>\<scene>@windows.png. A second RenderTargetBitmap in one process
// wedges headless (the first always succeeds, the second always hangs), so the
// capture script drives ONE scene per process via `--capture-scene <id>`; without
// that flag every enabled scene renders in the one process (fine on a real
// desktop, wedges headless). Each run exits 0 (rendered) or 1 (failed).
//
// The scenes reuse the same fixtures (PreviewData) and pure builders (WelcomeView,
// AlbumExpansionRows, AlbumCardVisual) the component gallery and the production
// window render — no re-implemented view content. A scene that would need a live
// library handle is absent from the registry, not faked.
//
// The capture runs the real app so RenderTargetBitmap has a live visual tree, but
// Program.Main skips the Velopack hooks and single-instance redirect in capture
// mode, and App.OnLaunched branches here before any library startup, so it never
// touches the keychain or ~/.bae. Every stage appends to <outdir>\capture.log
// because a WinExe has no visible stderr. Compiled only in DEBUG builds.
internal static class ShotCapture
{
    // The command-line flag App.OnLaunched checks. The output directory is the
    // argument that follows it (the capture script creates it before launch).
    internal const string Flag = "--capture-shots";

    // Optional flag selecting a single scene by id (the argument that follows it),
    // so the script can run one scene per process. Absent → all enabled scenes.
    internal const string SceneFlag = "--capture-scene";

    // The platform suffix in the shared "<scene>@<platform>.png" gallery contract.
    private const string Platform = "windows";

    // A WinExe has no visible stderr, so every capture stage appends to
    // <outdir>/capture.log; the script prints it after the run. Null (and every
    // Log a no-op) outside capture mode.
    private static string? _logPath;

    // Per-wait timeouts. Each wait is bounded and logged so a stall names its
    // stage instead of eating the script's global timeout.
    private const int LoadedTimeoutMs = 15000;
    private const int FrameTimeoutMs = 5000;
    private const int RenderTimeoutMs = 20000;
    private const int PixelsTimeoutMs = 10000;

    // Point the log at <outputDir>\capture.log and record process entry. Called
    // from Program.Main before any startup plumbing.
    internal static void BeginCapture(string outputDir)
    {
        _logPath = Path.Combine(outputDir, "capture.log");
        Log("process entry (Program.Main)");
    }

    // Append one timestamped line, swallowing any error — logging must never break
    // a capture. A no-op until BeginCapture sets the path.
    internal static void Log(string message)
    {
        var path = _logPath;
        if (path is null)
        {
            return;
        }

        try
        {
            File.AppendAllText(path, $"{DateTime.UtcNow:HH:mm:ss.fff} {message}{Environment.NewLine}");
        }
        catch (Exception)
        {
            // A failed log write must not abort the capture.
        }
    }

    // One capture scene: a stable id (the gallery scene key, shared across
    // platforms) and a fixed logical size, with a builder that renders the
    // composition against fixtures. The builder receives the capture window's
    // HWND for the scene (story 3) whose shell binds the system transport controls
    // to it at construction. Disabled scenes stay staged but produce no PNG (a
    // deliberate, honest gap in the gallery).
    private readonly record struct Scene(
        string Id, double Width, double Height, Func<IntPtr, FrameworkElement> Build, bool Enabled = true);

    // The scene registry: desktop-story ids (notes/desktop-stories.md) — the
    // gallery is the stories' per-platform verification sheet. Each enabled scene
    // renders through a pure builder over fixtures — none needs a live library
    // handle. The empty-library shell (story 3) composes over AppService.Stubbed:
    // every domain service is a fail-loud stub except an empty LibraryService, so
    // the shell renders its content off that stub alone and never reaches a handle.
    private static IReadOnlyList<Scene> Scenes { get; } = new[]
    {
        new Scene("story-1-first-run", 900, 600, BuildWelcome),
        new Scene("story-3-empty-library", 1350, 850, BuildStory3EmptyLibrary),
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

    // The single-scene id from --capture-scene, or null when the flag is absent
    // (render all enabled scenes). Not a hard error when malformed — the caller
    // (the script) always supplies a valid id, and a null falls back to all.
    internal static string? GetSceneArg(IReadOnlyList<string> args)
    {
        for (var i = 0; i < args.Count; i++)
        {
            if (args[i] == SceneFlag && i + 1 < args.Count && !string.IsNullOrEmpty(args[i + 1]))
            {
                return args[i + 1];
            }
        }

        return null;
    }

    // Render the requested scene(s) to <outputDir>\<id>@windows.png, then exit the
    // process. sceneId names one scene (one scene per process, the headless-safe
    // path the script drives); null renders every enabled scene in this process.
    // The exit code is 0 only when every rendered scene succeeded.
    internal static async Task RunAsync(string outputDir, string? sceneId)
    {
        Log(sceneId is null ? "RunAsync begin (all enabled scenes)" : $"RunAsync begin (scene={sceneId})");
        var failed = false;
        var rendered = 0;
        try
        {
            var folder = await StorageFolder.GetFolderFromPathAsync(outputDir);
            Log($"output folder resolved: {outputDir}");
            foreach (var scene in Scenes)
            {
                if (!scene.Enabled || (sceneId is not null && scene.Id != sceneId))
                {
                    continue;
                }
                rendered++;
                try
                {
                    await CaptureAsync(scene, folder);
                    Log($"scene '{scene.Id}': done");
                }
                catch (Exception exception)
                {
                    failed = true;
                    Log($"scene '{scene.Id}': FAILED: {exception}");
                }
            }

            if (rendered == 0)
            {
                failed = true;
                Log($"no scene rendered (requested '{sceneId}' is unknown or disabled)");
            }
        }
        catch (Exception exception)
        {
            failed = true;
            Log($"RunAsync fatal: {exception}");
        }

        Log($"RunAsync exiting with code {(failed ? 1 : 0)}");
        Environment.Exit(failed ? 1 : 0);
    }

    private static async Task CaptureAsync(Scene scene, StorageFolder folder)
    {
        Log($"scene '{scene.Id}': build start");
        // Create the window first so its HWND is available to the scene builder:
        // the story-3 shell's AppService binds the system transport controls to it
        // at construction, exactly like the production window. Benign for this
        // throwaway capture window — it is closed as soon as the render is read.
        var window = new Window();
        var windowHandle = WinRT.Interop.WindowNative.GetWindowHandle(window);
        var root = new Grid
        {
            Width = scene.Width,
            Height = scene.Height,
            // A solid page background so the PNG is opaque rather than the
            // window's Mica; the app runs dark-themed (set in App's constructor),
            // so this resolves to the dark background.
            Background = (Brush)Application.Current.Resources["ApplicationPageBackgroundThemeBrush"],
        };
        root.Children.Add(scene.Build(windowHandle));
        Log($"scene '{scene.Id}': content built");

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
        Log($"scene '{scene.Id}': window activated, scale={scale}");

        try
        {
            if (await AwaitOr(loaded.Task, LoadedTimeoutMs, $"scene '{scene.Id}' Loaded"))
            {
                Log($"scene '{scene.Id}': Loaded fired");
            }

            root.UpdateLayout();
            // Let the compositor commit one frame before reading pixels back. A
            // miss here is non-fatal — the render still runs and the blank check
            // catches a dead compositor.
            await AwaitOr(WaitForNextFrameAsync(), FrameTimeoutMs, $"scene '{scene.Id}' frame tick");

            Log($"scene '{scene.Id}': render start");
            var bitmap = new RenderTargetBitmap();
            if (!await AwaitOr(bitmap.RenderAsync(root).AsTask(), RenderTimeoutMs, $"scene '{scene.Id}' RenderAsync"))
            {
                throw new TimeoutException("RenderAsync did not complete");
            }

            var pixelsTask = bitmap.GetPixelsAsync().AsTask();
            if (!await AwaitOr(pixelsTask, PixelsTimeoutMs, $"scene '{scene.Id}' GetPixelsAsync"))
            {
                throw new TimeoutException("GetPixelsAsync did not complete");
            }

            var pixelBuffer = await pixelsTask;
            var pixels = new byte[pixelBuffer.Length];
            using (var reader = DataReader.FromBuffer(pixelBuffer))
            {
                reader.ReadBytes(pixels);
            }

            var blank = IsAllZero(pixels);
            Log($"scene '{scene.Id}': pixels read: {pixels.Length} bytes, {bitmap.PixelWidth}x{bitmap.PixelHeight}, allZero={blank}");
            if (blank)
            {
                // A blank render (nothing realized / dead compositor) must fail the
                // scene, not write an empty PNG that passes the existence check.
                throw new InvalidOperationException("render produced all-zero (blank) pixels");
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

            Log($"scene '{scene.Id}': PNG written");
        }
        finally
        {
            window.Close();
        }
    }

    // Await a task but give up after timeoutMs, logging the stage that stalled.
    // Returns whether the task completed; a completed task's exception is observed
    // (re-thrown) here so the scene's own catch reports it.
    private static async Task<bool> AwaitOr(Task task, int timeoutMs, string stage)
    {
        var winner = await Task.WhenAny(task, Task.Delay(timeoutMs));
        if (winner == task)
        {
            await task;
            return true;
        }

        Log($"{stage}: TIMED OUT after {timeoutMs}ms");
        return false;
    }

    // Whether the pixel buffer is entirely zero — a transparent-black (blank)
    // render. A real opaque scene has 0xFF alpha bytes, so this discriminates a
    // dead render from a rendered one. An empty buffer is blank too.
    private static bool IsAllZero(byte[] pixels)
    {
        foreach (var value in pixels)
        {
            if (value != 0)
            {
                return false;
            }
        }

        return true;
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
    // than a re-implementation of it. The window handle is unused (the welcome
    // chooser has no transport controls to bind).
    private static FrameworkElement BuildWelcome(IntPtr windowHandle)
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

    // The empty-library shell (desktop story 3): the production MainView over an
    // AppService.Stubbed whose LibraryService reports zero counts and empty pages.
    // The view's constructor loads the browse content off that stub, which drives
    // the real zero-count empty-state branch — "no albums" over the docked idle
    // now-playing bar. The session has no handle and every other service is a
    // fail-loud stub, so nothing on this path reaches a live library.
    private static FrameworkElement BuildStory3EmptyLibrary(IntPtr windowHandle)
    {
        var dispatcher = DispatcherQueue.GetForCurrentThread();
        var session = new SessionStore(dispatcher);
        var appService = AppService.Stubbed(session, dispatcher, () => windowHandle, EmptyLibrary());
        return new MainView(appService, session, () => Task.CompletedTask, () => windowHandle);
    }

    // A LibraryService whose album/composer/artist counts are zero and whose pages
    // are empty — the reads LoadCurrentBrowserMode makes to render the empty state.
    // The (true, ...) currency flag marks the library present-but-empty (not a gone
    // handle), so the browser shows the empty state rather than skipping the load.
    // Every other read stays a fail-loud stub.
    private static LibraryService EmptyLibrary() => new()
    {
        AlbumCount = () => (true, 0L),
        AlbumPage = (_, _, _) => (true, (new List<Album>(), (string?)null)),
        ComposerCount = () => (true, 0L),
        ComposerPage = (_, _, _) => (true, (new List<ComposerSummary>(), (string?)null)),
        ArtistCount = () => (true, 0L),
        ArtistPage = (_, _, _) => (true, (new List<ArtistSummary>(), (string?)null)),
    };
}
#endif
