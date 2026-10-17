// Original CS Demo Manager requests are preserved at their existing call sites.
// This adapter sends only implemented contracts to the Tauri Rust backend.
type Listener = (payload: unknown) => void;

function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const native = (window as any).__TAURI_INTERNALS__;
  if (!native?.invoke) return Promise.reject(new Error('This renderer requires the DemoStrike Tauri host.'));
  return native.invoke(command, args);
}

export class WebSocketClient {
  private listeners = new Map<string, Set<Listener>>();

  public constructor(onConnectionSuccess: () => void, _onConnectionError: (event: CloseEvent) => void) {
    // Tauri IPC has no socket handshake. The first request verifies the backend.
    queueMicrotask(onConnectionSuccess);
  }

  public on(name: string, listener: Listener) {
    const listeners = this.listeners.get(name) ?? new Set<Listener>();
    listeners.add(listener);
    this.listeners.set(name, listeners);
  }

  public off(name: string, listener: Listener) {
    this.listeners.get(name)?.delete(listener);
  }

  public removeAllEventListeners(name: string) {
    this.listeners.delete(name);
  }

  public send(message: { name: string; payload?: unknown }) {
    const request = invoke('csdm_request', { name: message.name, payload: message.payload });
    if (message.name === 'connect-database') {
      // The original database loader expects an error payload for this request.
      return request.catch((error) => ({ code: 1, message: String(error) }));
    }
    return request;
  }
}
