import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import PetCanvas from "../components/PetCanvas";
import { PetCreatorPanel } from "./PetCreatorPanel";
import type { AppConfig, PetDetailDto, PetListItem } from "../types/aipet";

const PREVIEW_SCALE = 64 / 192;

interface LibraryProps {
  onActivePetChanged?: () => void;
}

export function PetLibrarySection({ onActivePetChanged }: LibraryProps) {
  const [pets, setPets] = useState<PetListItem[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [activePetId, setActivePetId] = useState("");
  const [detail, setDetail] = useState<PetDetailDto | null>(null);
  const [sheetUrl, setSheetUrl] = useState("");
  const [appCfg, setAppCfg] = useState<AppConfig | null>(null);
  const [petsDir, setPetsDir] = useState("");
  const [loadingDetail, setLoadingDetail] = useState(false);
  const [showCreator, setShowCreator] = useState(false);

  const refresh = useCallback(async () => {
    await invoke("refresh_pets_dir");
    const list = await invoke<PetListItem[]>("list_pets");
    const cfg = await invoke<AppConfig>("get_app_config");
    const root = await invoke<string>("get_app_data_path");
    setPets(list);
    setPetsDir(`${root}\\pets\\`);
    setActivePetId(cfg.activePetId);
    setAppCfg(cfg);
    setSelectedId((prev) => {
      if (prev && list.some((p) => p.folderId === prev)) return prev;
      if (cfg.activePetId && list.some((p) => p.folderId === cfg.activePetId)) {
        return cfg.activePetId;
      }
      return list[0]?.folderId ?? null;
    });
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const unlisten = listen<AppConfig>("app-config-changed", (e) => {
      setActivePetId(e.payload.activePetId);
      setAppCfg(e.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (!selectedId) {
      setDetail(null);
      setSheetUrl("");
      return;
    }
    let cancelled = false;
    setLoadingDetail(true);
    void (async () => {
      try {
        const d = await invoke<PetDetailDto>("get_pet_detail", {
          folderId: selectedId,
        });
        const path = await invoke<string>("get_pet_spritesheet_path", {
          folderId: selectedId,
        });
        if (cancelled) return;
        setDetail(d);
        setSheetUrl(convertFileSrc(path));
      } catch {
        if (!cancelled) {
          setDetail(null);
          setSheetUrl("");
        }
      } finally {
        if (!cancelled) setLoadingDetail(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [selectedId]);

  const canvasRows = useMemo(
    () =>
      detail?.atlas.rows.map((r) => ({
        state: r.state,
        displayName: r.displayName,
        frames: r.frames,
      })) ?? [],
    [detail],
  );

  const summonPet = useCallback(async () => {
    if (!selectedId || !appCfg) return;
    await invoke("save_app_config", {
      config: { ...appCfg, activePetId: selectedId },
    });
    setActivePetId(selectedId);
    onActivePetChanged?.();
  }, [selectedId, appCfg, onActivePetChanged]);

  const [deleting, setDeleting] = useState(false);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);

  const deletePet = useCallback(async () => {
    if (!selectedId || selectedId === activePetId) return;
    setShowDeleteConfirm(true);
  }, [selectedId, activePetId]);

  const confirmDelete = useCallback(async () => {
    if (!selectedId) return;
    setShowDeleteConfirm(false);
    setDeleting(true);
    try {
      await invoke("delete_pet", { folderId: selectedId });
      setSelectedId(null);
      await refresh();
    } catch (e) {
      void e;
    } finally {
      setDeleting(false);
    }
  }, [selectedId, refresh]);

  const isActive = selectedId === activePetId;

  return (
    <div className="flex min-h-[520px] flex-col">
      <div className="mb-4 flex shrink-0 items-center justify-between">
        <h2 className="text-xl font-bold">宠物库</h2>
        <button
          type="button"
          className="rounded-lg border border-gray-200 px-3 py-1 text-sm hover:bg-gray-50"
          onClick={() => void refresh()}
        >
          刷新列表
        </button>
      </div>

      <p className="mb-3 shrink-0 break-all text-xs text-gray-500">
        资源目录：{petsDir || "加载中…"}
      </p>

      {pets.length === 0 ? (
        <p className="text-sm text-gray-500">暂无宠物，请向 pets 目录添加资源或点击下方按钮生成。</p>
      ) : (
        <>
          <div className="mb-4 flex shrink-0 gap-3 overflow-x-auto pb-2">
            {pets.map((p) => {
              const selected = p.folderId === selectedId;
              return (
                <button
                  key={p.folderId}
                  type="button"
                  className={`flex shrink-0 flex-col items-center rounded-lg border-2 p-2 transition-colors ${
                    selected
                      ? "border-pink-500 bg-pink-50"
                      : "border-gray-200 hover:border-pink-200"
                  }`}
                  onClick={() => setSelectedId(p.folderId)}
                >
                  <PetThumb spritesheetPath={p.spritesheetPath} />
                  <span className="mt-1 max-w-[80px] truncate text-xs font-medium">
                    {p.displayName}
                  </span>
                  {p.folderId === activePetId ? (
                    <span className="mt-0.5 text-[10px] text-pink-600">使用中</span>
                  ) : null}
                </button>
              );
            })}
          </div>

          {selectedId ? (
            <div className="flex min-h-0 flex-1 gap-4">
              <div className="max-h-[420px] min-h-0 flex-1 overflow-y-auto rounded-lg border border-gray-200 p-3">
                {loadingDetail || !detail || !sheetUrl ? (
                  <p className="text-sm text-gray-500">加载动作预览…</p>
                ) : (
                  <div className="grid grid-cols-3 gap-3">
                    {detail.atlas.rows.map((row, i) => (
                      <div
                        key={row.state}
                        className="flex items-center gap-2 rounded-lg border border-gray-100 bg-gray-50 p-2"
                      >
                        <div
                          className="shrink-0 overflow-hidden rounded"
                          style={{
                            width: Math.round(detail.atlas.cellWidth * PREVIEW_SCALE),
                            height: Math.round(detail.atlas.cellHeight * PREVIEW_SCALE),
                          }}
                        >
                          <PetCanvas
                            rows={canvasRows}
                            currentRowIndex={i}
                            spritesheetUrl={sheetUrl}
                            cellWidth={detail.atlas.cellWidth}
                            cellHeight={detail.atlas.cellHeight}
                            animationSpeed={appCfg?.animationSpeed ?? 1}
                            scale={PREVIEW_SCALE}
                          />
                        </div>
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-xs font-medium">
                            {row.displayName}
                          </p>
                          <p className="text-[10px] text-gray-400">{row.frames} 帧</p>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              <div className="flex w-56 shrink-0 flex-col gap-4">
                <div>
                  <h3 className="mb-2 text-sm font-semibold text-gray-800">
                    {detail?.manifest.displayName ?? "…"}
                  </h3>
                  <p className="text-sm leading-relaxed text-gray-600">
                    {detail?.manifest.description || "暂无描述"}
                  </p>
                </div>
                <button
                  type="button"
                  disabled={isActive}
                  className="rounded-lg bg-pink-500 px-4 py-2 text-sm font-medium text-white hover:bg-pink-600 disabled:cursor-not-allowed disabled:bg-gray-300"
                  onClick={() => void summonPet()}
                >
                  {isActive ? "当前使用中" : "召唤此宠物"}
                </button>
                <button
                  type="button"
                  disabled={isActive || deleting}
                  className="rounded-lg border border-red-200 px-4 py-2 text-sm text-red-500 hover:bg-red-50 disabled:cursor-not-allowed disabled:border-gray-200 disabled:text-gray-300"
                  onClick={() => void deletePet()}
                >
                  {deleting ? "删除中..." : "放生此宠物"}
                </button>
              </div>
            </div>
          ) : null}
        </>
      )}

      <div className="mt-auto shrink-0 pt-6">
        <button
          type="button"
          className="flex w-full items-center justify-center gap-2 rounded-xl border-2 border-dashed border-pink-300 bg-pink-50/50 px-4 py-3 text-sm font-medium text-pink-600 transition-colors hover:border-pink-400 hover:bg-pink-100/60"
          onClick={() => setShowCreator(true)}
        >
          <span className="text-lg leading-none">+</span>
          <span>捏一个新宠物</span>
        </button>
      </div>

      {showCreator && (
        <PetCreatorPanel
          onClose={() => setShowCreator(false)}
          onCreated={() => void refresh()}
        />
      )}

      {showDeleteConfirm && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
          <div className="w-[340px] rounded-2xl bg-white p-6 shadow-2xl">
            <h3 className="mb-3 text-center text-base font-bold text-gray-800">确认放生</h3>
            <p className="mb-5 text-center text-sm text-gray-600">
              确定要放生「{detail?.manifest.displayName ?? selectedId}」吗？<br />
              <span className="text-xs text-red-400">此操作不可恢复</span>
            </p>
            <div className="flex justify-center gap-3">
              <button
                type="button"
                className="rounded-lg border border-gray-300 px-4 py-2 text-sm hover:bg-gray-50"
                onClick={() => setShowDeleteConfirm(false)}
              >
                取消
              </button>
              <button
                type="button"
                className="rounded-lg bg-red-500 px-4 py-2 text-sm font-medium text-white hover:bg-red-600"
                onClick={() => void confirmDelete()}
              >
                确认放生
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function PetThumb({ spritesheetPath }: { spritesheetPath: string }) {
  return (
    <div
      className="overflow-hidden rounded bg-gray-100"
      style={{ width: 48, height: 52 }}
    >
      <div
        style={{
          width: 192,
          height: 208,
          transform: "scale(0.25)",
          transformOrigin: "top left",
          backgroundImage: `url("${convertFileSrc(spritesheetPath)}")`,
          backgroundRepeat: "no-repeat",
          backgroundPosition: "0 0",
        }}
      />
    </div>
  );
}
