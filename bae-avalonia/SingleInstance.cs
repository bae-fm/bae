using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Pipes;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace Bae.Desktop;

// One running instance per edition, with the second launch forwarding its argv
// to the first instead of opening a second window.
//
// Primary election is a named mutex (Win32 named mutex on Windows, a
// /tmp-backed named mutex on Unix). Forwarding is a named pipe — a true named
// pipe on Windows, a Unix-domain-socket-backed pipe under the temp dir on Linux
// (.NET's System.IO.Pipes). The second instance connects, writes its argv, and
// exits; the primary's listener hands the argv to the app. A launch whose
// handoff fails takes over when the running copy is gone. Both names carry an
// edition key (bae / baeium) so the two never redirect into each other, and a
// per-user token (see UserScope) so a shared multi-user host can't let one user
// squat or intercept another's channel through the world-writable temp dir.
internal sealed class SingleInstance : IDisposable
{
    private readonly string _pipeName;
    private readonly Mutex _mutex;
    private readonly CancellationTokenSource _cts = new();

    private SingleInstance(string pipeName, Mutex mutex)
    {
        _pipeName = pipeName;
        _mutex = mutex;
    }

    // Become the primary instance, or forward this launch's argv to the running
    // primary and return null (the caller then exits). The primary starts a
    // listener that invokes onActivation for each forwarded launch.
    public static SingleInstance? Acquire(
        string edition, IReadOnlyList<string> args, Action<IReadOnlyList<string>> onActivation)
    {
        var name = Name(edition);
        var mutex = new Mutex(initiallyOwned: true, name, out var isPrimary);
        if (!isPrimary && (Forward(name, args) || !TakeOver(mutex)))
        {
            mutex.Dispose();
            return null;
        }

        var instance = new SingleInstance(name, mutex);
        instance.Listen(onActivation);
        return instance;
    }

    // The mutex and pipe name for an edition and this user.
    internal static string Name(string edition) => $"bae-single-instance-{edition}-{UserScope()}";

    // After a failed handoff: become the primary when the running copy is gone
    // — it exited, abandoning the mutex, or released it on its way out — and
    // leave this launch to exit when that copy still holds it.
    private static bool TakeOver(Mutex mutex)
    {
        bool owned;
        try
        {
            owned = mutex.WaitOne(0);
        }
        catch (AbandonedMutexException)
        {
            owned = true;
        }
        if (owned)
        {
            BaeDiagnostics.Logger.Info("The running copy is gone; this launch takes over.");
        }
        else
        {
            BaeDiagnostics.Logger.Error(
                "The running copy didn't take this launch and is still running; this launch exits.");
        }
        return owned;
    }

    // Accept forwarded launches one at a time, handing each one's argv on. A
    // server instance accepts one connection, so the next instance is opened
    // as soon as one connects, before that launch is read: a launch arriving
    // meanwhile connects to it instead of to an instance about to close, which
    // would take its argv and drop it. The mutex (not the pipe) is what elects
    // the primary, so no gap between instances can hand primacy to a third
    // launch.
    private void Listen(Action<IReadOnlyList<string>> onActivation) =>
        _ = Task.Run(async () =>
        {
            NamedPipeServerStream? next = null;
            try
            {
                while (!_cts.IsCancellationRequested)
                {
                    try
                    {
                        next ??= OpenServer();
                        await next.WaitForConnectionAsync(_cts.Token);
                    }
                    catch (Exception exception) when (exception is not OperationCanceledException)
                    {
                        // The waiting instance failed: a fresh one takes the next launch.
                        next?.Dispose();
                        next = null;
                        await Recover(exception);
                        continue;
                    }
                    using var connected = next;
                    next = null;
                    try
                    {
                        next = OpenServer();
                        using var reader = new StreamReader(connected, Encoding.UTF8);
                        var payload = await reader.ReadToEndAsync(_cts.Token);
                        onActivation(payload.Split('\n', StringSplitOptions.RemoveEmptyEntries));
                    }
                    catch (Exception exception) when (exception is not OperationCanceledException)
                    {
                        await Recover(exception);
                    }
                }
            }
            catch (OperationCanceledException)
            {
                // Disposed: the app is exiting.
            }
            finally
            {
                next?.Dispose();
            }
        });

    // A launch that hung up mid-handoff is logged. Anything else is a bug,
    // reported; listening goes on so later launches still reach this copy, and
    // the pause keeps a failure that repeats from spinning.
    private async Task Recover(Exception exception)
    {
        if (exception is IOException)
        {
            BaeDiagnostics.Logger.Warning("A forwarded launch broke off.", exception);
            return;
        }
        BaeDiagnostics.Logger.Error("The single-instance listener failed; listening again.", exception);
        BaeCrashReporting.CaptureException(exception);
        await Task.Delay(TimeSpan.FromSeconds(1), _cts.Token);
    }

    // One server instance; two are open at once while a connected launch is
    // read and the next waits.
    private NamedPipeServerStream OpenServer() =>
        new(_pipeName, PipeDirection.In, 2, PipeTransmissionMode.Byte, PipeOptions.Asynchronous);

    // A per-user token folded into the mutex and pipe names. The BCL named pipe
    // backs onto a socket in the world-writable temp dir on Unix
    // (/tmp/CoreFxPipe_<name>) under a predictable name; without a per-user
    // component another local user could pre-create that name to block this
    // user's primary election, or stand up a listener to receive another
    // user's forwarded argv. Derived from the user's home directory — stable across launches
    // and not spoofable through the USERNAME env var the way Environment.UserName
    // is. (On Windows the mutex already lives in the per-session namespace; the
    // scope is belt-and-suspenders there.)
    private static string UserScope()
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var hash = System.Security.Cryptography.SHA256.HashData(Encoding.UTF8.GetBytes(home));
        return Convert.ToHexString(hash, 0, 6).ToLowerInvariant();
    }

    // Hand this launch's argv to the running copy; false, logged, when it
    // didn't take it — gone, or not answering.
    private static bool Forward(string pipeName, IReadOnlyList<string> args)
    {
        try
        {
            using var client = new NamedPipeClientStream(".", pipeName, PipeDirection.Out);
            client.Connect(2000);
            var payload = Encoding.UTF8.GetBytes(string.Join('\n', args));
            client.Write(payload, 0, payload.Length);
            client.Flush();
            return true;
        }
        catch (Exception exception) when (exception is IOException or TimeoutException)
        {
            BaeDiagnostics.Logger.Warning("Couldn't hand this launch to the running copy.", exception);
            return false;
        }
    }

    public void Dispose()
    {
        _cts.Cancel();
        _cts.Dispose();
        _mutex.Dispose();
    }
}
