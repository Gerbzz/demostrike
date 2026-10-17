// A user-selected radar stays in this WebView session only. It is never
// written to the library or bundled with the renderer.
let selected: { mapName: string; file: File } | null = null;

export function rememberSelectedRadar(mapName: string, file: File) {
  selected = { mapName, file };
}

export function selectedRadarFile(mapName: string): File | null {
  return selected?.mapName === mapName ? selected.file : null;
}
