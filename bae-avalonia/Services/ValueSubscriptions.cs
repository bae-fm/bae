using System;
using System.Collections.Generic;
using Avalonia.Threading;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>Owns the typed value subscriptions for one open library.</summary>
internal sealed class ValueSubscriptions : IDisposable
{
    private readonly List<LiveSubscription> _subscriptions = new();

    public ValueSubscriptions(
        SessionStore session,
        Dispatcher dispatcher,
        SettingsStore settings,
        SyncStatusStore sync,
        PlaybackStore playback,
        StorageStore storage,
        CastStore cast,
        ImportStore import,
        Action<string, string> showError)
    {
        session.WithCurrentHandle(handle =>
        {
            _subscriptions.Add(handle.SubscribeConfig(new ConfigSink((config, syncReady) =>
                dispatcher.Post(() => settings.Apply(NativeBae.SettingsFromConfig(handle, config, syncReady))))));
            _subscriptions.Add(handle.SubscribeSyncStatus(new SyncSink(value =>
                dispatcher.Post(() => sync.Apply(value)))));
            _subscriptions.Add(handle.SubscribeQueue(new QueueSink(
                value => dispatcher.Post(() => playback.ApplyQueueValue(value)),
                error => dispatcher.Post(() => Show(error, showError)))));
            _subscriptions.Add(handle.SubscribeOutbox(new OutboxSink(
                value => dispatcher.Post(() => storage.ApplyOutbox(value)),
                error => dispatcher.Post(() => Show(error, showError)))));
            _subscriptions.Add(handle.SubscribeDownloads(new DownloadSink(value =>
                dispatcher.Post(() => storage.ApplyDownloads(value)))));
            _subscriptions.Add(handle.SubscribeOutputs(new OutputSink(value =>
                dispatcher.Post(() => storage.ApplyOutputs(value)))));
            _subscriptions.Add(handle.SubscribeCastDevices(new CastSink(value =>
                dispatcher.Post(() => cast.ApplyDevices(value)))));
            _subscriptions.Add(handle.SubscribeImportCandidates(new ImportSink(value =>
                dispatcher.Post(() => import.ApplyCandidates(value)))));
            _subscriptions.Add(handle.SubscribeImportTriage(new ImportTriageSink(
                value => dispatcher.Post(() => import.ApplyTriage(value)),
                error => dispatcher.Post(() => Show(error, showError)))));
        });
    }

    private static void Show(BridgeException error, Action<string, string> showError)
    {
        if (BridgeDisplay.LocalizedLine(error) is { } line)
        {
            showError(Loc.Chrome("error.title"), line);
        }
    }

    public void Dispose()
    {
        foreach (var subscription in _subscriptions)
        {
            subscription.Cancel();
            subscription.Dispose();
        }
        _subscriptions.Clear();
    }

    private sealed class ConfigSink(Action<BridgeConfig, bool> apply) : ConfigCallback
    {
        public void OnValue(BridgeConfig config, bool syncReady) => apply(config, syncReady);
    }

    private sealed class SyncSink(Action<BridgeSyncStatusSnapshot> apply) : SyncStatusCallback
    {
        public void OnValue(BridgeSyncStatusSnapshot value) => apply(value);
    }

    private sealed class QueueSink(Action<BridgeQueueSnapshot> apply, Action<BridgeException> error) : QueueCallback
    {
        public void OnValue(BridgeQueueSnapshot value) => apply(value);
        public void OnError(BridgeException value) => error(value);
    }

    private sealed class OutboxSink(Action<BridgeOutboxSnapshot> apply, Action<BridgeException> error) : OutboxCallback
    {
        public void OnValue(BridgeOutboxSnapshot value) => apply(value);
        public void OnError(BridgeException value) => error(value);
    }

    private sealed class DownloadSink(Action<BridgeDownloadSnapshot> apply) : DownloadCallback
    {
        public void OnValue(BridgeDownloadSnapshot value) => apply(value);
    }

    private sealed class OutputSink(Action<BridgeOutputSnapshot> apply) : OutputCallback
    {
        public void OnValue(BridgeOutputSnapshot value) => apply(value);
    }

    private sealed class CastSink(Action<BridgeCastDevice[]> apply) : CastDevicesCallback
    {
        public void OnValue(BridgeCastDevice[] devices) => apply(devices);
    }

    private sealed class ImportSink(Action<BridgeImportCandidatesSnapshot> apply) : ImportCandidatesCallback
    {
        public void OnValue(BridgeImportCandidatesSnapshot value) => apply(value);
    }

    private sealed class ImportTriageSink(
        Action<BridgeTriageQueue> apply,
        Action<BridgeException> error) : ImportTriageCallback
    {
        public void OnValue(BridgeTriageQueue value) => apply(value);
        public void OnError(BridgeException value) => error(value);
    }
}
