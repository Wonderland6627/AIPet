import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AppConfig,
  PetDetailDto,
  StateConfig,
  SystemState,
} from "../types/aipet";
import { resolvePetStatus } from "../utils/triggerResolver";

export function PetStatusPanel() {
  const [system, setSystem] = useState<SystemState | null>(null);
  const [stateConfig, setStateConfig] = useState<StateConfig | null>(null);
  const [detail, setDetail] = useState<PetDetailDto | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [dragDirection, setDragDirection] = useState<"left" | "right" | null>(null);
  const dragTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadConfig = useCallback(async () => {
    const cfg = await invoke<AppConfig>("get_app_config");
    const sc = await invoke<StateConfig>("get_state_config");
    setStateConfig(sc);
    if (!cfg.activePetId) {
      setDetail(null);
      return;
    }
    try {
      const d = await invoke<PetDetailDto>("get_pet_detail", {
        folderId: cfg.activePetId,
      });
      setDetail(d);
    } catch {
      setDetail(null);
    }
  }, []);

  useEffect(() => {
    void loadConfig();
  }, [loadConfig]);

  useEffect(() => {
    const unlistenSys = listen<SystemState>("system-state-changed", (e) => {
      setSystem(e.payload);
    });
    const unlistenCfg = listen<AppConfig>("app-config-changed", () => {
      void loadConfig();
    });
    const unlistenState = listen<StateConfig>("state-config-changed", (e) => {
      setStateConfig(e.payload);
    });
    const unlistenDrag = listen<{ dragging: boolean; direction: "left" | "right" | null }>(
      "pet-dragging-changed",
      (e) => {
        if (e.payload.dragging) {
          setDragDirection(e.payload.direction);
          dragTimerRef.current = setTimeout(() => setIsDragging(true), 150);
        } else {
          if (dragTimerRef.current) {
            clearTimeout(dragTimerRef.current);
            dragTimerRef.current = null;
          }
          setIsDragging(false);
          setDragDirection(null);
        }
      },
    );
    return () => {
      unlistenSys.then((fn) => fn());
      unlistenCfg.then((fn) => fn());
      unlistenState.then((fn) => fn());
      unlistenDrag.then((fn) => fn());
    };
  }, [loadConfig]);

  if (!system || !stateConfig) {
    return (
      <div className="mt-auto border-t border-gray-200 pt-4">
        <p className="px-3 text-xs font-medium text-gray-500">当前状态</p>
        <p className="px-3 py-2 text-xs text-gray-400">加载中…</p>
      </div>
    );
  }

  const atlasRows = detail?.atlas.rows ?? [];
  const status = resolvePetStatus(system, stateConfig, atlasRows);

  if (isDragging) {
    return (
      <div className="mt-auto border-t border-gray-200 pt-4">
        <p className="px-3 text-xs font-medium text-gray-500">当前状态</p>
        <div className="px-3 py-2 text-xs text-gray-700">
          <p className="mb-1">
            <span className="text-gray-500">正在：</span>
            <span className="font-medium text-pink-700">
              {dragDirection === "left"
                ? "向左拖拽"
                : dragDirection === "right"
                  ? "向右拖拽"
                  : "正在被拖拽"}
            </span>
          </p>
          <p className="mb-2 text-gray-600">
            鼠标拖拽宠物窗口中{dragDirection ? `（方向：${dragDirection}）` : ""}
          </p>
          <p className="text-gray-400">
            CPU {Math.round(system.cpuPercent)}% · 内存{" "}
            {Math.round(system.memoryPercent)}%
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="mt-auto border-t border-gray-200 pt-4">
      <p className="px-3 text-xs font-medium text-gray-500">当前状态</p>
      <div className="px-3 py-2 text-xs text-gray-700">
        <p className="mb-1">
          <span className="text-gray-500">正在：</span>
          <span className="font-medium text-pink-700">{status.displayName}</span>
        </p>
        <p className="mb-2 break-words text-gray-600">{status.reason}</p>
        <p className="text-gray-400">
          CPU {Math.round(system.cpuPercent)}% · 内存{" "}
          {Math.round(system.memoryPercent)}%
        </p>
        {system.activeProcess ? (
          <p className="mt-1 truncate text-gray-400" title={system.activeProcess}>
            进程 {system.activeProcess}
          </p>
        ) : null}
      </div>
    </div>
  );
}
