import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import type { AppConfig } from "../types/aipet";

export function AppSettingsSection() {
  const [cfg, setCfg] = useState<AppConfig | null>(null);
  const [dataPath, setDataPath] = useState("");
  const [appVersion, setAppVersion] = useState("");

  const load = useCallback(async () => {
    const c = await invoke<AppConfig>("get_app_config");
    const root = await invoke<string>("get_app_data_path");
    const ver = await getVersion().catch(() => "");
    setCfg(c);
    setDataPath(root);
    setAppVersion(ver);
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    const u = listen<AppConfig>("app-config-changed", (e) => {
      setCfg(e.payload);
    });
    return () => {
      u.then((fn) => fn());
    };
  }, []);

  const save = useCallback(
    async (patch: Partial<AppConfig>) => {
      if (!cfg) return;
      const next = { ...cfg, ...patch };
      setCfg(next);
      await invoke("save_app_config", { config: next });
    },
    [cfg],
  );

  if (!cfg) {
    return <p className="text-sm text-gray-500">加载设置…</p>;
  }

  return (
    <div className="flex min-h-full flex-col">
      <div>
        <h2 className="mb-4 text-xl font-bold">应用设置</h2>
        <div className="space-y-6">
          <label className="flex items-center justify-between py-2">
            <span className="text-sm">宠物置顶</span>
            <input
              type="checkbox"
              checked={cfg.alwaysOnTop}
              className="h-4 w-4 accent-pink-500"
              onChange={(e) => void save({ alwaysOnTop: e.target.checked })}
            />
          </label>
          <label className="flex items-center justify-between py-2">
            <span className="text-sm">开机自启</span>
            <input
              type="checkbox"
              checked={cfg.autoStart}
              className="h-4 w-4 accent-pink-500"
              onChange={(e) => void save({ autoStart: e.target.checked })}
            />
          </label>
          <div>
            <div className="mb-1 flex justify-between text-sm">
              <span>动画速度</span>
              <span className="text-gray-500">{cfg.animationSpeed.toFixed(1)}×</span>
            </div>
            <input
              type="range"
              min={0.1}
              max={3}
              step={0.1}
              value={cfg.animationSpeed}
              className="w-full accent-pink-500"
              onChange={(e) =>
                void save({ animationSpeed: Number(e.target.value) })
              }
            />
          </div>
          <div>
            <div className="mb-1 flex justify-between text-sm">
              <span>动画尺寸</span>
              <span className="text-gray-500">{cfg.animationScale.toFixed(1)}×</span>
            </div>
            <input
              type="range"
              min={0.5}
              max={3}
              step={0.1}
              value={cfg.animationScale}
              className="w-full accent-pink-500"
              onChange={(e) =>
                void save({ animationScale: Number(e.target.value) })
              }
            />
          </div>
          <div>
            <p className="mb-2 text-sm">宠物位置</p>
            <button
              type="button"
              className="rounded-lg border border-gray-300 px-3 py-1.5 text-sm hover:bg-gray-50"
              onClick={() => void invoke("reset_main_window_position")}
            >
              重置宠物位置（右下角）
            </button>
          </div>
          <div className="rounded-lg border border-gray-200 p-3">
            <p className="mb-2 text-xs text-gray-500">配置目录</p>
            <p className="mb-3 break-all font-mono text-xs text-gray-700">
              {dataPath || "加载中…"}
            </p>
            <button
              type="button"
              className="rounded-lg border border-gray-300 px-3 py-1.5 text-sm hover:bg-gray-50"
              onClick={() => void invoke("open_app_data_dir")}
            >
              打开配置目录
            </button>
          </div>
          <p className="text-xs text-gray-400">
            当前展示宠物由「宠物库」设为当前宠物后保存在配置中。
          </p>
        </div>
      </div>
      <p className="mt-auto pt-8 text-right text-xs text-gray-400">
        版本 v{appVersion || "—"}
      </p>
    </div>
  );
}
