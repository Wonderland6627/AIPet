import { useState, useEffect, useCallback, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import PetCanvas from "./components/PetCanvas";

const PROCESS_ROW_MAP: Record<string, number> = {
  "cursor.exe": 7,
  "code.exe": 7,
  "unity.exe": 7,
  "devenv.exe": 7,
  "excel.exe": 6,
  "wps.exe": 6,
  "feishu.exe": 0,
  "wechat.exe": 0,
};

function mapProcessToRow(processName: string): number {
  return PROCESS_ROW_MAP[processName.toLowerCase()] ?? 0;
}

export default function App() {
  const [processRow, setProcessRow] = useState(0);
  const [isDragging, setIsDragging] = useState(false);
  const clickCountRef = useRef(0);
  const clickTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const currentRow = isDragging ? 1 : processRow;

  useEffect(() => {
    const unlisten = listen<string>("active-process-changed", (event) => {
      setProcessRow(mapProcessToRow(event.payload));
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const openSettings = useCallback(async () => {
    const settings = await WebviewWindow.getByLabel("settings");
    if (settings) {
      await settings.show();
      await settings.setFocus();
    }
  }, []);

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    invoke("show_pet_menu");
  }, []);

  const handleMouseDown = useCallback(
    async (e: React.MouseEvent) => {
      if (e.button !== 0) return;

      clickCountRef.current += 1;
      if (clickCountRef.current >= 2) {
        clickCountRef.current = 0;
        if (clickTimerRef.current) clearTimeout(clickTimerRef.current);
        openSettings();
        return;
      }

      clickTimerRef.current = setTimeout(() => {
        clickCountRef.current = 0;
      }, 400);

      setIsDragging(true);
      try {
        await getCurrentWindow().startDragging();
      } finally {
        setIsDragging(false);
      }
    },
    [openSettings],
  );

  return (
    <div
      style={{
        width: 192,
        height: 208,
        pointerEvents: "auto",
        cursor: isDragging ? "grabbing" : "grab",
      }}
      onMouseDown={handleMouseDown}
      onContextMenu={handleContextMenu}
    >
      <PetCanvas currentRow={currentRow} />
    </div>
  );
}
