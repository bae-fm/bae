using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Pipes;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace Bae.Desktop;

// One running instance per edition, with the second launch forwarding its
// activation (a bae:// URL or file/folder args) to the first instead of opening a
// second window — the cross-platform stand-in for WinUI's AppInstance redirect.
//
// Primary election is a named mutex (Win32 named mutex on Windows, a
// /tmp-backed named mutex on Unix). Forwarding is a named pipe — a true named
// pipe on Windows, a Unix-domain-socket-backed pipe under the temp dir on Linux
// (.NET's System.IO.Pipes). The second instance connects, writes its argv, and
// exits; the primary's listener parses the argv into an ActivationIntent and
// hands it to the coordinator. One key per edition (bae / baeium) so the two
// never redirect into each other.
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
        string edition, IReadOnlyList<string> args, Action<ActivationIntent?> onActivation)
    {
        var name = $"bae-single-instance-{edition}";
        var mutex = new Mutex(initiallyOwned: true, name, out var isPrimary);
        if (!isPrimary)
        {
            Forward(name, args);
            mutex.Dispose();
            return null;
        }

        var instance = new SingleInstance(name, mutex);
        instance.Listen(onActivation);
        return instance;
    }

    // Accept forwarded launches one at a time, parsing each into an intent. A
    // single-instance server accepts one connection, so the loop recreates the
    // server after each; the mutex (not the pipe) is what elects the primary, so
    // the gap between accepting and recreating can't hand primacy to a third
    // launch.
    private void Listen(Action<ActivationIntent?> onActivation) =>
        _ = Task.Run(async () =>
        {
            while (!_cts.IsCancellationRequested)
            {
                try
                {
                    using var server = new NamedPipeServerStream(
                        _pipeName, PipeDirection.In, 1, PipeTransmissionMode.Byte, PipeOptions.Asynchronous);
                    await server.WaitForConnectionAsync(_cts.Token);
                    using var reader = new StreamReader(server, Encoding.UTF8);
                    var payload = await reader.ReadToEndAsync(_cts.Token);
                    var args = payload.Split('\n', StringSplitOptions.RemoveEmptyEntries);
                    var intent = ActivationIntentModel.Parse(args, Directory.Exists);
                    onActivation(intent);
                }
                catch (OperationCanceledException)
                {
                    return;
                }
                catch (IOException exception)
                {
                    BaeDiagnostics.Logger.Warning("Single-instance listener error.", exception);
                }
            }
        });

    private static void Forward(string pipeName, IReadOnlyList<string> args)
    {
        try
        {
            using var client = new NamedPipeClientStream(".", pipeName, PipeDirection.Out);
            client.Connect(2000);
            var payload = Encoding.UTF8.GetBytes(string.Join('\n', args));
            client.Write(payload, 0, payload.Length);
            client.Flush();
        }
        catch (Exception exception) when (exception is IOException or TimeoutException)
        {
            // The primary went away between the mutex check and the connect. The
            // launch is lost; a fresh mutex acquisition on the next launch will
            // elect a new primary. Nothing to surface.
        }
    }

    public void Dispose()
    {
        _cts.Cancel();
        _cts.Dispose();
        _mutex.Dispose();
    }
}
