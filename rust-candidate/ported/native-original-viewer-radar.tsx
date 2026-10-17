import React, { createContext, useContext } from 'react';
import type { InteractiveCanvas } from 'csdm/ui/hooks/use-interactive-map-canvas';

const RadarContext = createContext<ImageBitmap | null>(null);

export function NativeRadarProvider({ bitmap, children }: { bitmap: ImageBitmap | null; children: React.ReactNode }) {
  return <RadarContext.Provider value={bitmap}>{children}</RadarContext.Provider>;
}

// The pinned CSDM canvas and map transforms remain in use. Only its Electron
// file:// radar loader is replaced: selected artwork stays in WebView memory.
export function useDrawMapRadar() {
  const bitmap = useContext(RadarContext);

  const drawMapRadar = (context: CanvasRenderingContext2D, canvas: InteractiveCanvas) => {
    const x = canvas.zoomedX(0);
    const y = canvas.zoomedY(0);
    const size = canvas.getScaledRadarSize();
    if (bitmap) {
      context.drawImage(bitmap, x, y, size, size);
      return;
    }
    context.fillStyle = '#1c2228';
    context.fillRect(x, y, size, size);
    context.strokeStyle = '#36404a';
    context.lineWidth = 1;
    for (let index = 0; index <= 16; index++) {
      const offset = size * index / 16;
      context.beginPath();
      context.moveTo(x + offset, y);
      context.lineTo(x + offset, y + size);
      context.moveTo(x, y + offset);
      context.lineTo(x + size, y + offset);
      context.stroke();
    }
  };
  return { drawMapRadar };
}
