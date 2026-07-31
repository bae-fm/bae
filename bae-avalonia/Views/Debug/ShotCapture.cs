#if DEBUG
using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using Avalonia.Media.Imaging;
using Avalonia.Styling;
using Avalonia.Threading;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// DEBUG-only screenshot capture: `bae-avalonia --capture-shots <dir>` renders
// scenes to <dir>/<scene>@<platform>.png off-screen. The headless Skia platform
// renders every scene in one process — no compositor, no window activation, no
// per-scene process split — so this runs on any runner including ubuntu CI.
// `--capture-scene <id>` renders one scene; absent, every enabled scene renders.
// `--capture-variant dark|light` picks the OS appearance to force (default dark,
// the macOS-comparison reference). Each run exits 0 (rendered) or 1 (failed).
//
// The scenes reuse the same fixtures (PreviewData) and the production view
// builders (WelcomeView) the app renders — no re-implemented view content. A
// scene that would need a live library handle is absent from the registry, not
// faked. Every stage appends to <outdir>/capture.log because a GUI process has no
// visible stderr.
internal static class ShotCapture
{
    internal const string Flag = "--capture-shots";
    internal const string SceneFlag = "--capture-scene";
    internal const string VariantFlag = "--capture-variant";

    // The platform suffix in the shared "<scene>@<platform>.png" gallery contract.
    internal static readonly string Platform =
        OperatingSystem.IsWindows() ? "windows"
        : OperatingSystem.IsLinux() ? "linux"
        : "macos";

    private static string? _logPath;

    // One capture scene: a stable id (the gallery scene key, shared across
    // platforms) and a fixed logical size, with a builder that renders the
    // composition against fixtures.
    private readonly record struct Scene(string Id, int Width, int Height, Func<Control> Build);

    // The scene registry: desktop-story ids (notes/desktop-stories.md). Each scene
    // renders through a production builder over fixtures — none needs a live
    // library handle.
    private static IReadOnlyList<Scene> Scenes { get; } = new[]
    {
        new Scene("story-1-first-run", 900, 600, BuildWelcome),
        new Scene("story-3-empty-library", 1350, 850, BuildEmptyLibrary),
        new Scene(
            "import-release-queue",
            900,
            700,
            () => BuildImportQueue(
                PreviewData.ImportQueue,
                BridgeTriageTab.Ready,
                collapseGroup: false,
                refreshingRoot: null)),
        new Scene(
            "import-release-ambiguity-narrow",
            520,
            700,
            () => BuildImportQueue(
                PreviewData.ImportQueue,
                BridgeTriageTab.NeedsYou,
                collapseGroup: false,
                refreshingRoot: null)),
        new Scene(
            "import-release-queue-collapsed",
            900,
            700,
            () => BuildImportQueue(
                PreviewData.ImportQueue,
                BridgeTriageTab.Ready,
                collapseGroup: true,
                refreshingRoot: null)),
        new Scene(
            "import-release-scanning-refresh",
            900,
            700,
            () => BuildImportQueue(
                PreviewData.ImportScanningQueue,
                BridgeTriageTab.Ready,
                collapseGroup: false,
                refreshingRoot: PreviewData.ImportRoot)),
        new Scene(
            "import-release-resolved-reversal",
            900,
            700,
            () => BuildImportQueue(
                PreviewData.ImportResolvedQueue,
                BridgeTriageTab.Ready,
                collapseGroup: false,
                refreshingRoot: null)),
    };

    // True when args carry the capture flag; then outputDir is the directory that
    // follows it. Throws when the flag is present without a directory — a
    // malformed capture request fails loudly rather than falling through to the
    // real app.
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

    private static string? OptionAfter(IReadOnlyList<string> args, string flag)
    {
        for (var i = 0; i < args.Count; i++)
        {
            if (args[i] == flag && i + 1 < args.Count && !string.IsNullOrEmpty(args[i + 1]))
            {
                return args[i + 1];
            }
        }

        return null;
    }

    // Render the requested scene(s) to <outputDir>/<id>@<platform>.png, then exit.
    // Runs on the headless Skia platform set up by the caller (Program.Main).
    internal static int Run(IReadOnlyList<string> args, string outputDir)
    {
        _logPath = Path.Combine(outputDir, "capture.log");
        Directory.CreateDirectory(outputDir);
        Log("capture begin");

        var sceneId = OptionAfter(args, SceneFlag);
        var variant = OptionAfter(args, VariantFlag) == "light" ? ThemeVariant.Light : ThemeVariant.Dark;
        Application.Current!.RequestedThemeVariant = variant;
        Log($"variant={variant}, requested scene={sceneId ?? "(all)"}");

        var failed = false;
        var rendered = 0;
        foreach (var scene in Scenes)
        {
            if (sceneId is not null && scene.Id != sceneId)
            {
                continue;
            }
            rendered++;
            try
            {
                Capture(scene, outputDir);
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
            Log($"no scene rendered (requested '{sceneId}' is unknown)");
        }

        Log($"capture exiting with code {(failed ? 1 : 0)}");
        return failed ? 1 : 0;
    }

    private static void Capture(Scene scene, string outputDir)
    {
        Log($"scene '{scene.Id}': build start");
        var root = new Border
        {
            Width = scene.Width,
            Height = scene.Height,
        };
        // A solid page background so the PNG is opaque rather than transparent.
        root[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeBackgroundBrush");
        root.Child = scene.Build();

        var window = new Window
        {
            SystemDecorations = SystemDecorations.None,
            Width = scene.Width,
            Height = scene.Height,
            Content = root,
        };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        Log($"scene '{scene.Id}': laid out");

        var frame = window.CaptureRenderedFrame()
            ?? throw new InvalidOperationException("headless frame capture returned null");
        var path = Path.Combine(outputDir, $"{scene.Id}@{Platform}.png");
        using (var stream = File.Create(path))
        {
            frame.Save(stream);
        }
        window.Close();
        Log($"scene '{scene.Id}': PNG written to {path} ({frame.PixelSize.Width}x{frame.PixelSize.Height})");
    }

    // Append one timestamped line, swallowing any error — logging must never break
    // a capture.
    private static void Log(string message)
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
        catch (IOException)
        {
            // A failed log write must not abort the capture.
        }
    }

    // The first-run welcome chooser, staged with fixture libraries and no-op
    // callbacks. Drives WelcomeView's real content — the production path — rather
    // than a re-implementation of it.
    private static Control BuildWelcome() =>
        new WelcomeView(
            setStatus: _ => { },
            loadLibraries: () => PreviewData.WelcomeLibraries,
            createLibrary: _ => null,
            openLibrary: _ => { },
            showJoinLibrary: () => Task.CompletedTask,
            showRestoreFromCloud: () => Task.CompletedTask);

    // The empty-library shell (desktop story 3): the production shell chrome over
    // an AppService.Stubbed whose LibraryService reports zero counts. The shell
    // reads that zero-count empty-state branch off the stub, so the shot exercises
    // the shipped composition — the session has no handle and every other service
    // is a fail-loud stub, so nothing on this path reaches a live library.
    private static Control BuildEmptyLibrary()
    {
        var session = new SessionStore(Dispatcher.UIThread);
        var app = AppService.Stubbed(session, Dispatcher.UIThread, EmptyLibrary());
        var modalHost = new ModalHost();
        var lightbox = new LightboxOverlay();
        Func<Task> closeLibrary = () => Task.CompletedTask;
        Func<string, Task> switchLibrary = _ => Task.CompletedTask;
        return new MainShellView(
            app,
            new ReleaseActionDialogs(app, modalHost, lightbox),
            new ImportDialogs(modalHost, lightbox, app.Images, _ => Task.CompletedTask),
            new StorageDialog(app, modalHost),
            new SettingsWindow(app, new UpdateService(), closeLibrary, switchLibrary, () => Task.CompletedTask),
            new LibrariesDialog(app, modalHost, switchLibrary),
            closeLibrary);
    }

    private static Control BuildImportQueue(
        BridgeTriageQueue queue,
        BridgeTriageTab tab,
        bool collapseGroup,
        string? refreshingRoot)
    {
        var session = new SessionStore(Dispatcher.UIThread);
        var app = AppService.Stubbed(session, Dispatcher.UIThread, EmptyLibrary());
        var modalHost = new ModalHost();
        var lightbox = new LightboxOverlay();
        var view = new ImportSectionView(
            app,
            new ImportDialogs(modalHost, lightbox, app.Images, _ => Task.CompletedTask));
        if (collapseGroup)
        {
            app.ImportStore.Interaction.SetGroupExpanded(
                ImportStore.GroupDisclosureKey(
                    PreviewData.ImportGroupKey),
                false);
        }
        if (refreshingRoot is not null)
        {
            app.ImportStore.Interaction.SetRefreshing(refreshingRoot, true);
        }
        app.ImportStore.SeedPreview(
            queue,
            PreviewData.ImportWatchedFolders,
            tab);
        return view;
    }

    // A LibraryService whose album/composer/artist counts are zero and whose pages
    // are empty — the reads the shell makes to render the empty state. The
    // (true, ...) currency flag marks the library present-but-empty (not a gone
    // handle). Every other read stays a fail-loud stub.
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
