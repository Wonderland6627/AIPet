import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import type { AiApiConfig, AppConfig } from "../types/aipet";

function ToggleSwitch({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors ${checked ? "bg-pink-500" : "bg-gray-300"}`}
      onClick={() => onChange(!checked)}
    >
      <span
        className={`inline-block h-3.5 w-3.5 rounded-full bg-white shadow-sm transition-transform ${checked ? "translate-x-[18px]" : "translate-x-[3px]"}`}
      />
    </button>
  );
}

export function AppSettingsSection() {
  const [cfg, setCfg] = useState<AppConfig | null>(null);
  const [dataPath, setDataPath] = useState("");
  const [appVersion, setAppVersion] = useState("");
  const [showAiConfig, setShowAiConfig] = useState(false);

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
    <div className="flex min-h-full flex-col gap-5 pb-6">
      <h2 className="text-xl font-bold">应用设置</h2>

      {/* 宠物显示 */}
      <section className="rounded-xl border border-gray-100 bg-white p-4 shadow-sm">
        <h3 className="mb-3 text-sm font-semibold text-gray-800">宠物显示</h3>
        <div className="space-y-4">
          <label className="flex cursor-pointer items-center justify-between">
            <span className="text-sm text-gray-600">窗口置顶</span>
            <ToggleSwitch checked={cfg.alwaysOnTop} onChange={(v) => void save({ alwaysOnTop: v })} />
          </label>
          <div>
            <div className="mb-1.5 flex justify-between text-sm">
              <span className="text-gray-600">动画速度</span>
              <span className="tabular-nums text-gray-400">{cfg.animationSpeed.toFixed(1)}×</span>
            </div>
            <input
              type="range" min={0.1} max={3} step={0.1}
              value={cfg.animationSpeed}
              className="h-1.5 w-full cursor-pointer appearance-none rounded-full bg-gray-200 accent-pink-500"
              onChange={(e) => void save({ animationSpeed: Number(e.target.value) })}
            />
          </div>
          <div>
            <div className="mb-1.5 flex justify-between text-sm">
              <span className="text-gray-600">动画尺寸</span>
              <span className="tabular-nums text-gray-400">{cfg.animationScale.toFixed(1)}×</span>
            </div>
            <input
              type="range" min={0.5} max={3} step={0.1}
              value={cfg.animationScale}
              className="h-1.5 w-full cursor-pointer appearance-none rounded-full bg-gray-200 accent-pink-500"
              onChange={(e) => void save({ animationScale: Number(e.target.value) })}
            />
          </div>
          <div className="flex items-center justify-between">
            <span className="text-sm text-gray-600">宠物位置</span>
            <button
              type="button"
              className="rounded-lg border border-gray-200 px-3 py-1.5 text-xs text-gray-600 transition hover:border-pink-300 hover:text-pink-600"
              onClick={() => void invoke("reset_main_window_position")}
            >
              重置到右下角
            </button>
          </div>
        </div>
      </section>

      {/* AI 配置 */}
      <section className="rounded-xl border border-gray-100 bg-white p-4 shadow-sm">
        <h3 className="mb-3 text-sm font-semibold text-gray-800">AI 生图</h3>
        <p className="mb-3 text-xs text-gray-400">配置用于创建宠物精灵图的 AI 图像生成服务</p>
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-lg border border-gray-200 px-3 py-1.5 text-sm text-gray-600 transition hover:border-pink-300 hover:text-pink-600"
          onClick={() => setShowAiConfig(true)}
        >
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" className="h-4 w-4">
            <path fillRule="evenodd" d="M8.34 1.804A1 1 0 019.32 1h1.36a1 1 0 01.98.804l.295 1.473c.497.144.971.342 1.416.587l1.25-.834a1 1 0 011.262.125l.962.962a1 1 0 01.125 1.262l-.834 1.25c.245.445.443.919.587 1.416l1.473.294a1 1 0 01.804.98v1.362a1 1 0 01-.804.98l-1.473.295a6.95 6.95 0 01-.587 1.416l.834 1.25a1 1 0 01-.125 1.262l-.962.962a1 1 0 01-1.262.125l-1.25-.834a6.953 6.953 0 01-1.416.587l-.294 1.473a1 1 0 01-.98.804H9.32a1 1 0 01-.98-.804l-.295-1.473a6.957 6.957 0 01-1.416-.587l-1.25.834a1 1 0 01-1.262-.125l-.962-.962a1 1 0 01-.125-1.262l.834-1.25a6.957 6.957 0 01-.587-1.416l-1.473-.294A1 1 0 011 11.68v-1.362a1 1 0 01.804-.98l1.473-.295c.144-.497.342-.971.587-1.416l-.834-1.25a1 1 0 01.125-1.262l.962-.962A1 1 0 015.38 3.03l1.25.834a6.957 6.957 0 011.416-.587l.294-1.473zM13 10a3 3 0 11-6 0 3 3 0 016 0z" clipRule="evenodd" />
          </svg>
          配置 AI API
        </button>
      </section>

      {/* 系统 */}
      <section className="rounded-xl border border-gray-100 bg-white p-4 shadow-sm">
        <h3 className="mb-3 text-sm font-semibold text-gray-800">系统</h3>
        <div className="space-y-4">
          <label className="flex cursor-pointer items-center justify-between">
            <span className="text-sm text-gray-600">开机自启</span>
            <ToggleSwitch checked={cfg.autoStart} onChange={(v) => void save({ autoStart: v })} />
          </label>
          <div>
            <div className="mb-1.5 flex items-center justify-between">
              <span className="text-sm text-gray-600">配置目录</span>
              <button
                type="button"
                className="rounded-lg border border-gray-200 px-3 py-1.5 text-xs text-gray-600 transition hover:border-pink-300 hover:text-pink-600"
                onClick={() => void invoke("open_app_data_dir")}
              >
                打开
              </button>
            </div>
            <p className="break-all rounded-md bg-gray-50 px-2.5 py-1.5 font-mono text-xs text-gray-500">
              {dataPath || "加载中…"}
            </p>
          </div>
        </div>
      </section>

      {/* 版本 */}
      <p className="mt-auto text-center text-xs text-gray-300">
        AIPet v{appVersion || "—"}
      </p>

      {showAiConfig && <AiConfigModal onClose={() => setShowAiConfig(false)} />}
    </div>
  );
}

const PROVIDER_PRESETS: Record<string, { baseUrl: string; model: string; placeholder: string }> = {
  dashscope: {
    baseUrl: "https://dashscope.aliyuncs.com/api/v1",
    model: "wan2.7-image",
    placeholder: "sk-... (阿里云百炼 API Key)",
  },
  openai: {
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-image-1",
    placeholder: "sk-... (OpenAI API Key)",
  },
  gemini: {
    baseUrl: "https://generativelanguage.googleapis.com/v1beta",
    model: "gemini-2.5-flash-image",
    placeholder: "AIza... (Google Gemini API Key)",
  },
};

function AiConfigModal({ onClose }: { onClose: () => void }) {
  const [config, setConfig] = useState<AiApiConfig>({
    provider: "dashscope",
    apiKey: "",
    baseUrl: "https://dashscope.aliyuncs.com/api/v1",
    model: "wan2.7-image",
    providerCache: {},
  });
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");
  const [showApiKey, setShowApiKey] = useState(false);

  useEffect(() => {
    void invoke<AiApiConfig>("get_ai_api_config").then((c) =>
      setConfig({ ...c, providerCache: c.providerCache ?? {} }),
    );
  }, []);

  const handleProviderChange = (provider: string) => {
    setConfig((prev) => {
      const nextCache = { ...(prev.providerCache ?? {}) };
      nextCache[prev.provider] = {
        apiKey: prev.apiKey,
        baseUrl: prev.baseUrl,
        model: prev.model,
      };

      const cached = nextCache[provider];
      if (cached) {
        return {
          ...prev,
          provider,
          apiKey: cached.apiKey,
          baseUrl: cached.baseUrl,
          model: cached.model,
          providerCache: nextCache,
        };
      }

      const preset = PROVIDER_PRESETS[provider];
      if (!preset) {
        return { ...prev, provider, providerCache: nextCache };
      }
      return {
        ...prev,
        provider,
        apiKey: "",
        baseUrl: preset.baseUrl,
        model: preset.model,
        providerCache: nextCache,
      };
    });
  };

  const handleSave = async () => {
    setSaving(true);
    setMessage("");
    try {
      const payload: AiApiConfig = {
        ...config,
        providerCache: {
          ...(config.providerCache ?? {}),
          [config.provider]: {
            apiKey: config.apiKey,
            baseUrl: config.baseUrl,
            model: config.model,
          },
        },
      };
      await invoke("save_ai_api_config", { config: payload });
      setConfig(payload);
      setMessage("保存成功");
      setTimeout(() => setMessage(""), 2000);
    } catch (e) {
      setMessage(`保存失败: ${e}`);
    } finally {
      setSaving(false);
    }
  };

  const currentPreset = PROVIDER_PRESETS[config.provider];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-[440px] rounded-2xl bg-white p-6 shadow-2xl">
        <h3 className="mb-4 text-center text-lg font-bold">AI API 配置</h3>

        <div className="mb-4">
          <label className="mb-1 block text-sm text-gray-700">Provider</label>
          <select
            className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm"
            value={config.provider}
            onChange={(e) => handleProviderChange(e.target.value)}
          >
            <option value="dashscope">阿里云百炼 (DashScope)</option>
            <option value="openai">OpenAI</option>
            <option value="gemini">Google Gemini</option>
          </select>
        </div>

        <div className="mb-4">
          <label className="mb-1 block text-sm text-gray-700">API Key</label>
          <div className="relative">
            <input
              type={showApiKey ? "text" : "password"}
              className="w-full rounded-lg border border-gray-300 px-3 py-2 pr-9 text-sm"
              placeholder={currentPreset?.placeholder ?? "API Key"}
              value={config.apiKey}
              onChange={(e) =>
                setConfig({ ...config, apiKey: e.target.value })
              }
            />
            <button
              type="button"
              className="absolute right-2 top-1/2 -translate-y-1/2 flex h-6 w-6 items-center justify-center rounded text-gray-400 hover:text-gray-700"
              onClick={() => setShowApiKey(!showApiKey)}
              title={showApiKey ? "隐藏" : "显示"}
            >
              {showApiKey ? (
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" className="h-4 w-4">
                  <path fillRule="evenodd" d="M3.28 2.22a.75.75 0 00-1.06 1.06l14.5 14.5a.75.75 0 101.06-1.06l-1.745-1.745a10.029 10.029 0 003.3-4.38 1.651 1.651 0 000-1.185A10.004 10.004 0 009.999 3a9.956 9.956 0 00-4.744 1.194L3.28 2.22zM7.752 6.69l1.092 1.092a2.5 2.5 0 013.374 3.373l1.092 1.092a4 4 0 00-5.558-5.558z" clipRule="evenodd" />
                  <path d="M10.748 13.93l2.523 2.523A9.987 9.987 0 0110 17c-4.257 0-7.893-2.66-9.336-6.41a1.651 1.651 0 010-1.186A10.007 10.007 0 014.09 5.12L6.07 7.1A4 4 0 0010.748 13.93z" />
                </svg>
              ) : (
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" className="h-4 w-4">
                  <path d="M10 12.5a2.5 2.5 0 100-5 2.5 2.5 0 000 5z" />
                  <path fillRule="evenodd" d="M.664 10.59a1.651 1.651 0 010-1.186A10.004 10.004 0 0110 3c4.257 0 7.893 2.66 9.336 6.41.147.381.146.804 0 1.186A10.004 10.004 0 0110 17c-4.257 0-7.893-2.66-9.336-6.41zM14 10a4 4 0 11-8 0 4 4 0 018 0z" clipRule="evenodd" />
                </svg>
              )}
            </button>
          </div>
        </div>

        <div className="mb-4">
          <label className="mb-1 block text-sm text-gray-700">模型名称</label>
          <input
            type="text"
            className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm"
            placeholder={currentPreset?.model ?? "model-name"}
            value={config.model}
            onChange={(e) =>
              setConfig({ ...config, model: e.target.value })
            }
          />
          <p className="mt-1 text-xs text-gray-400">
            {config.provider === "dashscope" && "如 wan2.7-image, wan2.7-image-pro"}
            {config.provider === "openai" && "如 gpt-image-1"}
            {config.provider === "gemini" && "如 gemini-2.5-flash-image, gemini-3-pro-image-preview"}
          </p>
        </div>

        <div className="mb-4">
          <label className="mb-1 block text-sm text-gray-700">Base URL</label>
          <input
            type="text"
            className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm"
            placeholder={currentPreset?.baseUrl ?? "https://..."}
            value={config.baseUrl}
            onChange={(e) =>
              setConfig({ ...config, baseUrl: e.target.value })
            }
          />
          <p className="mt-1 text-xs text-gray-400">
            使用代理或自部署时可修改
          </p>
        </div>

        {message && (
          <p
            className={`mb-3 text-center text-xs ${message.includes("失败") ? "text-red-500" : "text-green-500"}`}
          >
            {message}
          </p>
        )}

        <div className="flex justify-end gap-3">
          <button
            type="button"
            className="rounded-lg border border-gray-300 px-4 py-2 text-sm hover:bg-gray-50"
            onClick={onClose}
          >
            关闭
          </button>
          <button
            type="button"
            disabled={saving}
            className="rounded-lg bg-pink-500 px-4 py-2 text-sm font-medium text-white hover:bg-pink-600 disabled:bg-gray-300"
            onClick={() => void handleSave()}
          >
            {saving ? "保存中..." : "保存"}
          </button>
        </div>
      </div>
    </div>
  );
}
