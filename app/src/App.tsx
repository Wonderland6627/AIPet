import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import PetCanvas from "./components/PetCanvas";
import { DEFAULT_FALLBACK_ROWS } from "./constants/codex";
import type {
  AppConfig,
  PetDetailDto,
  PetListItem,
  StateConfig,
  SystemState,
} from "./types/aipet";
import { resolveAnimationRowIndex } from "./utils/triggerResolver";

function rowsFromDetail(pet: PetDetailDto | null) {
  if (!pet) {
    return [...DEFAULT_FALLBACK_ROWS];
  }
  return pet.atlas.rows.map((r) => ({
    state: r.state,
    displayName: r.displayName,
    frames: r.frames,
  }));
}

export default function App() {
  const [appConfig, setAppConfig] = useState<AppConfig | null>(null);
  const [stateConfig, setStateConfig] = useState<StateConfig | null>(null);
  const [petDetail, setPetDetail] = useState<PetDetailDto | null>(null);
  const [spritesheetUrl, setSpritesheetUrl] = useState("/spritesheet.webp");
  const [processRow, setProcessRow] = useState(0);
  const [isDragging, setIsDragging] = useState(false);
  const [dragDirection, setDragDirection] = useState<"left" | "right" | null>(null);
  const isDraggingRef = useRef(false);
  const lastSystemRef = useRef<SystemState | null>(null);

  useEffect(() => {
    void emit("pet-dragging-changed", {
      dragging: isDragging,
      direction: isDragging ? dragDirection : null,
    });
  }, [isDragging, dragDirection]);

  const loadPetAssets = useCallback(async (folderId: string) => {
    if (!folderId) {
      setPetDetail(null);
      setSpritesheetUrl("/spritesheet.webp");
      return;
    }
    try {
      const detail = await invoke<PetDetailDto>("get_pet_detail", {
        folderId,
      });
      setPetDetail(detail);
      const path = await invoke<string>("get_pet_spritesheet_path", {
        folderId,
      });
      setSpritesheetUrl(convertFileSrc(path));
    } catch {
      setPetDetail(null);
      setSpritesheetUrl("/spritesheet.webp");
    }
  }, []);

  const applySystemState = useCallback(
    (system: SystemState) => {
      lastSystemRef.current = system;
      if (!stateConfig) return;
      const rows = rowsFromDetail(petDetail);
      setProcessRow(resolveAnimationRowIndex(system, stateConfig, rows));
    },
    [stateConfig, petDetail],
  );

  const applySystemStateRef = useRef(applySystemState);
  useEffect(() => {
    applySystemStateRef.current = applySystemState;
  }, [applySystemState]);

  const bootstrap = useCallback(async () => {
    let cfg = await invoke<AppConfig>("get_app_config");
    const pets = await invoke<PetListItem[]>("list_pets");
    if (!cfg.activePetId && pets.length > 0) {
      cfg = { ...cfg, activePetId: pets[0].folderId };
      await invoke("save_app_config", { config: cfg });
    }
    const sc = await invoke<StateConfig>("get_state_config");
    setAppConfig(cfg);
    setStateConfig(sc);
    await loadPetAssets(cfg.activePetId);
  }, [loadPetAssets]);

  useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  useEffect(() => {
    const unlisten = listen<AppConfig>("app-config-changed", (e) => {
      setAppConfig(e.payload);
      void loadPetAssets(e.payload.activePetId);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadPetAssets]);

  useEffect(() => {
    const unlisten = listen<StateConfig>("state-config-changed", (e) => {
      setStateConfig(e.payload);
      if (isDraggingRef.current) return;
      if (!lastSystemRef.current) return;
      applySystemState(lastSystemRef.current);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [applySystemState]);

  useEffect(() => {
    const unlisten = listen<SystemState>("system-state-changed", (event) => {
      if (isDraggingRef.current) return;
      applySystemState(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [applySystemState]);

  useEffect(() => {
    if (!stateConfig || !lastSystemRef.current || isDraggingRef.current) return;
    applySystemState(lastSystemRef.current);
  }, [stateConfig, petDetail, applySystemState]);

  const rows = useMemo(() => rowsFromDetail(petDetail), [petDetail]);
  const cellW = petDetail?.atlas.cellWidth ?? 192;
  const cellH = petDetail?.atlas.cellHeight ?? 208;
  const dragIdx = (() => {
    if (dragDirection === "left") {
      const leftIdx = rows.findIndex((r) => r.state === "running-left");
      if (leftIdx >= 0) return leftIdx;
    }
    const rightIdx = rows.findIndex((r) => r.state === "running-right");
    if (rightIdx >= 0) return rightIdx;
    const fallback = rows.findIndex((r) => r.state === "idle");
    return Math.max(0, fallback);
  })();
  const currentRow = isDragging ? dragIdx : processRow;

  const scale = appConfig?.animationScale ?? 1;
  const speed = appConfig?.animationSpeed ?? 1;

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    invoke("show_pet_menu");
  }, []);

  const dragCleanupRef = useRef<(() => void) | null>(null);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return;

    // 清理上一次未结束的拖拽（防御性）
    if (dragCleanupRef.current) {
      dragCleanupRef.current();
      dragCleanupRef.current = null;
    }

    let moveUnsub: (() => void) | null = null;
    let mouseUpHandler: (() => void) | null = null;
    let debounceTimer: ReturnType<typeof setTimeout> | null = null;
    let safetyTimer: ReturnType<typeof setTimeout> | null = null;
    let cleaned = false;
    let lastMoveX: number | null = null;

    const startDragState = () => {
      if (isDraggingRef.current) return;
      setIsDragging(true);
      isDraggingRef.current = true;
    };

    const cleanup = (shouldClamp: boolean) => {
      if (cleaned) return;
      cleaned = true;
      if (debounceTimer) clearTimeout(debounceTimer);
      if (safetyTimer) clearTimeout(safetyTimer);
      if (moveUnsub) moveUnsub();
      if (mouseUpHandler) {
        window.removeEventListener("mouseup", mouseUpHandler);
      }
      dragCleanupRef.current = null;

      if (isDraggingRef.current) {
        setIsDragging(false);
        isDraggingRef.current = false;
        setDragDirection(null);
        if (shouldClamp) {
          void invoke("clamp_main_window_to_screen");
        }
        if (lastSystemRef.current) {
          applySystemStateRef.current(lastSystemRef.current);
        }
      }
    };

    dragCleanupRef.current = () => cleanup(true);

    mouseUpHandler = () => {
      cleanup(true);
    };
    window.addEventListener("mouseup", mouseUpHandler);

    // 监听窗口移动事件 —— 每次移动重置去抖计时器
    getCurrentWindow()
      .listen("tauri://move", (moveEvent) => {
        startDragState();
        const { x } = moveEvent.payload as { x: number; y: number };
        if (lastMoveX !== null && x !== lastMoveX) {
          setDragDirection(x > lastMoveX ? "right" : "left");
        }
        lastMoveX = x;
        if (debounceTimer) clearTimeout(debounceTimer);
        debounceTimer = setTimeout(() => cleanup(true), 220);
      })
      .then((unsub) => {
        if (cleaned) {
          unsub();
          return;
        }
        moveUnsub = unsub;
      });

    // 安全超时：如果 500ms 内没有窗口移动事件，说明只是点击
    safetyTimer = setTimeout(() => {
      if (!isDraggingRef.current) {
        cleanup(false);
      }
    }, 500);

    // 启动 OS 拖拽
    void getCurrentWindow()
      .startDragging()
      .catch(() => cleanup(false));
  }, []);

  const wrapW = Math.round(cellW * scale);
  const wrapH = Math.round(cellH * scale);

  if (!appConfig || !stateConfig) {
    return (
      <div
        style={{
          width: 192,
          height: 208,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontSize: 11,
          color: "#999",
        }}
      >
        …
      </div>
    );
  }

  return (
    <div
      style={{
        width: wrapW,
        height: wrapH,
        pointerEvents: "auto",
        cursor: isDragging ? "grabbing" : "grab",
      }}
      onMouseDown={handleMouseDown}
      onContextMenu={handleContextMenu}
    >
      <PetCanvas
        rows={rows}
        currentRowIndex={currentRow}
        spritesheetUrl={spritesheetUrl}
        cellWidth={cellW}
        cellHeight={cellH}
        animationSpeed={speed}
        scale={scale}
      />
    </div>
  );
}
