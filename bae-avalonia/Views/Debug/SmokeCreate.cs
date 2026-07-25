#if DEBUG
using System;
using System.Collections.Generic;
using System.IO;
using Avalonia.Controls;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Threading;

namespace Bae.Desktop;

// DEBUG-only live-composition smoke: `--smoke-create <dir>` runs the real create
// flow end to end against the native bridge — startup, create a library, open its
// handle, compose the production AppService around it, and render the shell over
// that live (empty) library. Evidence the shipped composition opens on a
// genuinely created library, not a stub. Unlike --capture-shots this DOES touch
// the credential store and ~/.bae (it creates a real library), so it is a
// separate flag, run only on a throwaway machine.
internal static class SmokeCreate
{
    internal const string Flag = "--smoke-create";

    internal static bool TryGetOutputDir(IReadOnlyList<string> args, out string outputDir)
    {
        for (var i = 0; i < args.Count; i++)
        {
            if (args[i] == Flag && i + 1 < args.Count && !string.IsNullOrEmpty(args[i + 1]))
            {
                outputDir = args[i + 1];
                return true;
            }
        }

        outputDir = string.Empty;
        return false;
    }

    internal static int Run(string outputDir)
    {
        Directory.CreateDirectory(outputDir);
        var log = Path.Combine(outputDir, "smoke.log");
        void L(string message) => File.AppendAllText(log, $"{DateTime.UtcNow:HH:mm:ss.fff} {message}{Environment.NewLine}");

        try
        {
            BaeDiagnostics.Configure();
            NativeBae.Startup(BaeDiagnostics.Handle);
            L("bridge startup ok");

            var libraryId = LibraryDiscovery.Create(error => L($"create error: {error}"));
            if (libraryId is null)
            {
                L("create returned null");
                return 1;
            }
            L($"created library {libraryId}");

            var session = new SessionStore(Dispatcher.UIThread);
            var opened = session.OpenHandle(libraryId);
            L($"open handle: {opened}");
            if (opened != OpenHandleResult.Opened)
            {
                return 1;
            }

            var app = new AppService(session, Dispatcher.UIThread);
            var (current, albumCount) = app.Library.AlbumCount();
            L($"album count (current={current}) = {albumCount}");

            var root = new Border { Width = 1350, Height = 850, Child = new MainShellView(app, new ReleaseActionDialogs(app, new ModalHost(), new LightboxOverlay())) };
            root[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeBackgroundBrush");
            var window = new Window
            {
                SystemDecorations = SystemDecorations.None,
                Width = 1350,
                Height = 850,
                Content = root,
            };
            window.Show();
            Dispatcher.UIThread.RunJobs();

            var frame = Avalonia.Headless.HeadlessWindowExtensions.CaptureRenderedFrame(window)
                ?? throw new InvalidOperationException("headless frame capture returned null");
            var path = Path.Combine(outputDir, $"live-create-shell@{ShotCapture.Platform}.png");
            using (var stream = File.Create(path))
            {
                frame.Save(stream);
            }
            window.Close();
            L($"shell rendered over the created library to {path}");
            return 0;
        }
        catch (Exception exception)
        {
            L($"FAILED: {exception}");
            return 1;
        }
    }
}
#endif
