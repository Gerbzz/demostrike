import { defaultSettings } from '../upstream/cs-demo-manager/src/node/settings/default-settings';
import { mergeSettings } from './settings-merge.mjs';

type Native = { invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T> };
const native = (window as any).__TAURI_INTERNALS__ as Native | undefined;
const invoke = <T>(command: string, args: Record<string, unknown> = {}): Promise<T> =>
  native ? native.invoke<T>(command, args) : Promise.reject(new Error('The Tauri host is unavailable.'));
const unsupported = (name: string) => () => Promise.reject(new Error(`${name} has not been ported to the Rust backend.`));
const unlisten = () => () => {};

let currentSettings = defaultSettings;
const isMac = navigator.userAgent.includes('Macintosh');
const isWindows = navigator.userAgent.includes('Windows');
const api = {
  logger: console,
  ADDITIONAL_ARGUMENTS: [],
  WEB_SOCKET_SERVER_PORT: 0,
  IMAGES_FOLDER_PATH: '',
  embeddedDatabaseFolderPath: '',
  embeddedDatabaseLogFilePath: '',
  platform: isWindows ? 'win32' : isMac ? 'darwin' : 'linux',
  isMac, isWindows, isLinux: !isMac && !isWindows,
  getStartupArguments: async () => [],
  clearStartupArguments: async () => {},
  getTheme: async () => currentSettings.ui.theme,
  parseSettingsFile: async () => {
    currentSettings = await invoke<typeof defaultSettings>('csdm_load_settings', { defaults: defaultSettings });
    return currentSettings;
  },
  updateSettings: async (partial: Record<string, unknown>, options?: { preserveSourceArray?: boolean }) => {
    if ('database' in partial) {
      throw new Error('External CS Demo Manager database settings are not supported by this Tauri port.');
    }
    const settings = mergeSettings(currentSettings, partial, options?.preserveSourceArray === true);
    await invoke('csdm_save_settings', { settings });
    currentSettings = settings;
    return settings;
  },
  readTableState: async (name: string) => JSON.parse(localStorage.getItem(`table:${name}`) ?? '[]'),
  writeTableState: async (name: string, state: unknown) => localStorage.setItem(`table:${name}`, JSON.stringify(state)),
  hasUpdateReadyToInstall: async () => false,
  shouldShowChangelog: async () => false,
  isWindowMaximized: () => invoke('csdm_window_action', { action: 'is-maximized' }),
  maximizeWindow: () => invoke('csdm_window_action', { action: 'maximize' }),
  unMaximizeWindow: () => invoke('csdm_window_action', { action: 'unmaximize' }),
  minimizeWindow: () => invoke('csdm_window_action', { action: 'minimize' }),
  closeWindow: () => invoke('csdm_window_action', { action: 'close' }),
  canGoBack: async () => history.length > 1,
  canGoForward: async () => false,
  goBack: () => history.back(),
  goForward: () => history.forward(),
  onWindowClose: unlisten,
  onWindowMaximized: unlisten,
  onWindowUnMaximized: unlisten,
  onOpenDemoFile: unlisten,
  onOpenSettings: unlisten,
  onToggleSettingsVisibility: unlisten,
  onShowAbout: unlisten,
  onNavigateToBans: unlisten,
  onNavigateToPendingDownloads: unlisten,
  onUpdateDownloaded: unlisten,
  showTitleBarMenu: unsupported('Title-bar menu'),
  installUpdate: unsupported('Install update'),
  getWebFilePath: (_file: File) => { throw new Error('Demo file drop has not been ported.'); },
};

(window as any).csdm = new Proxy(api, {
  get(target, property: string) {
    if (property in target) return (target as any)[property];
    throw new Error(`CS Demo Manager preload method '${property}' has not been ported.`);
  },
});
