using System;
using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Routes core's invalidations to the handlers that reload from them. Static
// consumers (the album grid, sync status, import candidates) register once at
// startup; dialogs register while open and dispose the registration on close.
//
// BridgeInvalidation is a union base class with one nested type per kind, so
// registrations key on the kind's System.Type and Invalidate dispatches on the
// delivered instance's runtime type.
internal sealed class ProjectionRegistry
{
    private readonly Dictionary<Type, List<Action>> _handlers = new();

    public IDisposable Register(Type kind, Action onInvalidate)
    {
        if (!_handlers.TryGetValue(kind, out var handlers))
        {
            handlers = new List<Action>();
            _handlers[kind] = handlers;
        }

        handlers.Add(onInvalidate);
        return new Registration(this, kind, onInvalidate);
    }

    public void Invalidate(BridgeInvalidation invalidation)
    {
        if (!_handlers.TryGetValue(invalidation.GetType(), out var handlers))
        {
            return;
        }

        // Snapshot so a handler that registers or disposes while running doesn't
        // mutate the list mid-iteration.
        foreach (var handler in handlers.ToArray())
        {
            handler();
        }
    }

    private void Remove(Type kind, Action onInvalidate)
    {
        if (_handlers.TryGetValue(kind, out var handlers))
        {
            handlers.Remove(onInvalidate);
        }
    }

    private sealed class Registration : IDisposable
    {
        private readonly ProjectionRegistry _registry;
        private readonly Type _kind;
        private readonly Action _onInvalidate;
        private bool _disposed;

        public Registration(ProjectionRegistry registry, Type kind, Action onInvalidate)
        {
            _registry = registry;
            _kind = kind;
            _onInvalidate = onInvalidate;
        }

        public void Dispose()
        {
            if (_disposed)
            {
                return;
            }

            _disposed = true;
            _registry.Remove(_kind, _onInvalidate);
        }
    }
}
