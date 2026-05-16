import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";
import PetCanvas from "../components/PetCanvas";
import type { AppConfig, PetDetailDto, PetListItem } from "../types/aipet";

interface DetailProps {
  folderId: string | null;
  onClose: () => void;
  onSetActivePet: (folderId: string) => void;
}

function PetDetailDialog({
  folderId,
  onClose,
  onSetActivePet,
}: DetailProps) {
  const [detail, setDetail] = useState<PetDetailDto | null>(null);
  const [sheet, setSheet] = useState("");
  const [cfg, setCfg] = useState<AppConfig | null>(null);
  const [canvasRows, setCanvasRows] = useState<
    { state: string; displayName: string; frames: number }[]
  >([]);

  useEffect(() => {
    if (!folderId) {
      setDetail(null);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const d = await invoke<PetDetailDto>("get_pet_detail", { folderId });
        const path = await invoke<string>("get_pet_spritesheet_path", {
          folderId,
        });
        const c = await invoke<AppConfig>("get_app_config");
        if (cancelled) return;
        setDetail(d);
        setCanvasRows(
          d.atlas.rows.map((r) => ({
            state: r.state,
            displayName: r.displayName,
            frames: r.frames,
          })),
        );
        setSheet(convertFileSrc(path));
        setCfg(c);
      } catch {
        if (!cancelled) setDetail(null);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [folderId]);

  if (!folderId) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
      role="presentation"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="max-h-[90vh] w-full max-w-lg overflow-y-auto rounded-xl bg-white p-5 shadow-xl"
        role="dialog"
        aria-modal
      >
        <div className="mb-4 flex items-start justify-between gap-2">
          <div>
            <h3 className="text-lg font-bold">
              {detail?.manifest.displayName ?? "…"}
            </h3>
            <p className="mt-1 text-sm text-gray-600">
              {detail?.manifest.description ?? ""}
            </p>
          </div>
          <button
            type="button"
            className="rounded-lg px-2 py-1 text-sm text-gray-500 hover:bg-gray-100"
            onClick={onClose}
          >
            关闭
          </button>
        </div>

        {detail && sheet ? (
          <ul className="flex flex-col gap-4">
            {detail.atlas.rows.map((row, i) => (
              <li
                key={row.state}
                className="flex flex-wrap items-center gap-4 border-b border-gray-100 pb-4 last:border-0"
              >
                <div className="min-w-[120px] flex-1">
                  <div className="font-medium">{row.displayName}</div>
                  <div className="text-xs text-gray-400">{row.state}</div>
                </div>
                <div className="flex flex-col items-center gap-1">
                  <PetCanvas
                    rows={canvasRows}
                    currentRowIndex={i}
                    spritesheetUrl={sheet}
                    cellWidth={detail.atlas.cellWidth}
                    cellHeight={detail.atlas.cellHeight}
                    animationSpeed={cfg?.animationSpeed ?? 1}
                    scale={1}
                  />
                  <span className="text-xs text-gray-500">{row.frames} 帧</span>
                </div>
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-sm text-gray-500">加载中或资源缺失…</p>
        )}

        <div className="mt-6 flex justify-end gap-2">
          <button
            type="button"
            className="rounded-lg bg-pink-500 px-4 py-2 text-sm font-medium text-white hover:bg-pink-600"
            onClick={() => {
              if (folderId) onSetActivePet(folderId);
            }}
          >
            设为当前宠物
          </button>
        </div>
      </div>
    </div>
  );
}

interface LibraryProps {
  onActivePetChanged?: () => void;
}

export function PetLibrarySection({ onActivePetChanged }: LibraryProps) {
  const [pets, setPets] = useState<PetListItem[]>([]);
  const [detail, setDetail] = useState<string | null>(null);
  const [petsDir, setPetsDir] = useState("");

  const refresh = useCallback(async () => {
    await invoke("refresh_pets_dir");
    const list = await invoke<PetListItem[]>("list_pets");
    const root = await invoke<string>("get_app_data_path");
    setPets(list);
    setPetsDir(`${root}\\pets\\`);
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <div>
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-xl font-bold">宠物库</h2>
        <button
          type="button"
          className="rounded-lg border border-gray-200 px-3 py-1 text-sm hover:bg-gray-50"
          onClick={() => void refresh()}
        >
          刷新列表
        </button>
      </div>
      <p className="mb-4 break-all text-xs text-gray-500">
        资源目录：{petsDir || "加载中…"}
        <br />
        每个子文件夹需含 pet.json 与 spritesheet。
      </p>
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-3">
        {pets.map((p) => (
          <button
            key={p.folderId}
            type="button"
            className="rounded-lg border border-gray-200 p-3 text-left transition-shadow hover:shadow-md"
            onClick={() => setDetail(p.folderId)}
          >
            <div
              className="mx-auto mb-2 overflow-hidden rounded-lg bg-gray-100"
              style={{ width: 96, height: 104 }}
            >
              <div
                style={{
                  width: 192,
                  height: 208,
                  transform: "scale(0.5)",
                  transformOrigin: "top left",
                  backgroundImage: `url("${convertFileSrc(p.spritesheetPath)}")`,
                  backgroundRepeat: "no-repeat",
                  backgroundPosition: "0 0",
                  backgroundSize: "auto",
                }}
              />
            </div>
            <p className="text-center text-sm font-medium">{p.displayName}</p>
          </button>
        ))}
      </div>
      {pets.length === 0 ? (
        <p className="mt-6 text-sm text-gray-500">暂无宠物，请向 pets 目录添加资源。</p>
      ) : null}

      <PetDetailDialog
        folderId={detail}
        onClose={() => setDetail(null)}
        onSetActivePet={async (folderId) => {
          const c = await invoke<AppConfig>("get_app_config");
          await invoke("save_app_config", {
            config: { ...c, activePetId: folderId },
          });
          onActivePetChanged?.();
          setDetail(null);
        }}
      />
    </div>
  );
}
