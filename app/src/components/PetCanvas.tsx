import { useEffect } from "react";

const CELL_WIDTH = 192;
const CELL_HEIGHT = 208;
const FRAME_DURATION_MS = 120;

export const ROWS = [
  { state: "idle", frames: 6 },
  { state: "running-right", frames: 8 },
  { state: "running-left", frames: 8 },
  { state: "waving", frames: 4 },
  { state: "jumping", frames: 5 },
  { state: "failed", frames: 8 },
  { state: "waiting", frames: 6 },
  { state: "running", frames: 6 },
  { state: "review", frames: 6 },
] as const;

interface Props {
  currentRow: number;
  isLooping?: boolean;
}

export default function PetCanvas({ currentRow, isLooping = true }: Props) {
  useEffect(() => {
    if (document.getElementById("sprite-keyframes")) return;

    const style = document.createElement("style");
    style.id = "sprite-keyframes";
    style.textContent = ROWS.map(
      (row, i) =>
        `@keyframes sprite-row-${i}{from{background-position-x:0}to{background-position-x:-${row.frames * CELL_WIDTH}px}}`,
    ).join("\n");
    document.head.appendChild(style);

    return () => {
      style.remove();
    };
  }, []);

  const idx = currentRow >= 0 && currentRow < ROWS.length ? currentRow : 0;
  const row = ROWS[idx];
  const duration = row.frames * FRAME_DURATION_MS;

  return (
    <div
      key={idx}
      style={{
        width: CELL_WIDTH,
        height: CELL_HEIGHT,
        backgroundImage: "url(/spritesheet.webp)",
        backgroundRepeat: "no-repeat",
        backgroundPositionY: -(idx * CELL_HEIGHT),
        animation: `sprite-row-${idx} ${duration}ms steps(${row.frames}) ${isLooping ? "infinite" : "forwards"}`,
      }}
    />
  );
}
