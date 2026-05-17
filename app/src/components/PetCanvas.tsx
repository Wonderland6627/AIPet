import { useEffect, useMemo } from "react";

const BASE_FRAME_MS = 120;

export interface PetRow {
  state: string;
  displayName: string;
  frames: number;
}

interface Props {
  rows: readonly PetRow[];
  currentRowIndex: number;
  spritesheetUrl: string;
  cellWidth: number;
  cellHeight: number;
  animationSpeed: number;
  scale?: number;
  className?: string;
}

export default function PetCanvas({
  rows,
  currentRowIndex,
  spritesheetUrl,
  cellWidth,
  cellHeight,
  animationSpeed,
  scale = 1,
  className = "",
}: Props) {
  const styleId = useMemo(
    () =>
      `sk-${cellWidth}x${cellHeight}-${rows.map((r) => `${r.state}-${r.frames}`).join("_")}`,
    [rows, cellWidth, cellHeight],
  );

  useEffect(() => {
    const fullId = styleId;
    if (document.getElementById(fullId)) return;

    const style = document.createElement("style");
    style.id = fullId;
    style.textContent = rows
      .map(
        (row, i) =>
          `@keyframes sprite-row-${fullId}-${i}{from{transform:translate3d(0,0,0)}to{transform:translate3d(-${row.frames * cellWidth}px,0,0)}}`,
      )
      .join("\n");
    document.head.appendChild(style);
    return () => {
      style.remove();
    };
  }, [rows, cellWidth, styleId]);

  const idx =
    currentRowIndex >= 0 && currentRowIndex < rows.length ? currentRowIndex : 0;
  const row = rows[idx] ?? rows[0];
  if (!row) {
    return null;
  }

  const frameMs = BASE_FRAME_MS / Math.max(0.1, animationSpeed);
  const duration = row.frames * frameMs;
  const w = Math.round(cellWidth * scale);
  const h = Math.round(cellHeight * scale);
  const stripW = row.frames * cellWidth;

  const animName = `sprite-row-${styleId}-${idx}`;
  const anim = `${animName} ${duration}ms steps(${row.frames}) infinite`;

  return (
    <div className={className} style={{ width: w, height: h, overflow: "hidden" }}>
      <div
        style={{
          width: cellWidth,
          height: cellHeight,
          transform: `scale(${scale})`,
          transformOrigin: "top left",
          overflow: "hidden",
        }}
      >
        <div
          key={`${idx}-${spritesheetUrl}`}
          style={{
            width: stripW,
            height: cellHeight,
            backgroundImage: `url("${spritesheetUrl}")`,
            backgroundRepeat: "no-repeat",
            backgroundPositionY: -(idx * cellHeight),
            backgroundSize: "auto",
            imageRendering: "pixelated",
            willChange: "transform",
            animation: anim,
          }}
        />
      </div>
    </div>
  );
}
