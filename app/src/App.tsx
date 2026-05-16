import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
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
  const clickCountRef = useRef(0);
  const clickTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastSystemRef = useRef<SystemState | null>(null);

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
      setProcessRow(
        resolveAnimationRowIndex(system, stateConfig, rows),
      );
    },
    [stateConfig, petDetail],
  );

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
      if (lastSystemRef.current) {
        applySystemState(lastSystemRef.current);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [applySystemState]);

  useEffect(() => {
    const unlisten = listen<SystemState>("system-state-changed", (event) => {
      if (isDragging) return;
      applySystemState(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [applySystemState, isDragging]);

  useEffect(() => {
    if (!stateConfig || !lastSystemRef.current || isDragging) return;
    applySystemState(lastSystemRef.current);
  }, [stateConfig, petDetail, applySystemState, isDragging]);

  const rows = useMemo(() => rowsFromDetail(petDetail), [petDetail]);
  const cellW = petDetail?.atlas.cellWidth ?? 192;
  const cellH = petDetail?.atlas.cellHeight ?? 208;
  const dragRowIdx = rows.findIndex((r) => r.state === "running-right");
  const dragFallback = rows.findIndex((r) => r.state === "idle");
  const dragIdx = dragRowIdx >= 0 ? dragRowIdx : Math.max(0, dragFallback);
  const currentRow = isDragging ? dragIdx : processRow;

  const scale = appConfig?.animationScale ?? 1;
  const speed = appConfig?.animationSpeed ?? 1;

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
