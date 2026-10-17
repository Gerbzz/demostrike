import type { PlayerPosition } from 'csdm/common/types/player-position';
import { useViewerContext } from 'csdm/ui/match/viewer-2d/use-viewer-context';
import { useDrawPlayers as usePinnedDrawPlayers } from '../upstream/cs-demo-manager/src/ui/match/viewer-2d/drawing/use-draw-players';
import type { InteractiveCanvas } from 'csdm/ui/hooks/use-interactive-map-canvas';

// The pinned hook draws verified living players unchanged. A nullable
// m_bPawnIsAlive value cannot be represented by its boolean type, so these
// positions get a neutral hollow marker instead of an invented death.
export type NativeObservedPosition = PlayerPosition & { observedAlive: boolean | null };

export function useDrawPlayers() {
  const pinned = usePinnedDrawPlayers();
  const { playerPositions, currentTick } = useViewerContext();
  const drawPlayers = (context: CanvasRenderingContext2D, canvas: InteractiveCanvas) => {
    pinned.drawPlayers(context, canvas);
    for (const position of playerPositions as NativeObservedPosition[]) {
      if (position.tick !== currentTick || position.observedAlive !== null) continue;
      const x = canvas.zoomedToRadarX(position.x, position.z);
      const y = canvas.zoomedToRadarY(position.y, position.z);
      context.save();
      context.beginPath();
      context.arc(x, y, Math.max(4, canvas.zoomedSize(8)), 0, 2 * Math.PI);
      context.fillStyle = '#34404acc';
      context.strokeStyle = '#e0e5e9';
      context.lineWidth = 2;
      context.setLineDash([3, 2]);
      context.fill();
      context.stroke();
      context.restore();
    }
  };
  return { drawPlayers };
}
