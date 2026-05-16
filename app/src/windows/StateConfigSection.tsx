import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  CODEX_BASE_STATES,
  DRAG_BOUND_STATES,
  PRESET_PROCESSES,
  TRIGGER_TYPE_LABELS,
} from "../constants/codex";
import type {
  AppConfig,
  PetAtlasRow,
  PetDetailDto,
  StateConfig,
  StateMapping,
  TriggerConfig,
  TriggerType,
} from "../types/aipet";
import { normalizeExe, validateMappingUniqueness } from "../utils/triggerResolver";

type EditableRow = {
  state: string;
  label: string;
  isBase: boolean;
  supported: boolean;
  isDragBound: boolean;
};

function buildEditableRows(atlasRows: PetAtlasRow[]): EditableRow[] {
  const byState = new Map(atlasRows.map((r) => [r.state, r]));
  const baseSet = new Set(CODEX_BASE_STATES.map((b) => b.state));
  const base: EditableRow[] = CODEX_BASE_STATES.map((b) => ({
    state: b.state,
    label: b.label,
    isBase: true,
    supported: !!byState.get(b.state),
    isDragBound: DRAG_BOUND_STATES.has(b.state),
  }));
  const extras: EditableRow[] = atlasRows
    .filter((r) => !baseSet.has(r.state))
    .map((r) => ({
      state: r.state,
      label: r.displayName,
      isBase: false,
      supported: true,
      isDragBound: DRAG_BOUND_STATES.has(r.state),
    }));
  return [...base, ...extras];
}

function defaultTrigger(type: TriggerType): TriggerConfig {
  switch (type) {
    case "processFocus":
      return { type: "processFocus", processes: [] };
    case "highResource":
      return { type: "highResource", resource: "cpu", threshold: 90 };
    case "audioPlaying":
      return { type: "audioPlaying" };
    case "microphoneActive":
      return { type: "microphoneActive" };
    case "continuousFocus":
      return { type: "continuousFocus", minutes: 60 };
    case "computerIdle":
      return { type: "computerIdle", minutes: 5 };
    default:
      return { type: "processFocus", processes: [] };
  }
}

function mappingForState(
  mappings: StateMapping[],
  state: string,
): StateMapping | undefined {
  return mappings.find((m) => m.state === state);
}

export function StateConfigSection() {
  const [appCfg, setAppCfg] = useState<AppConfig | null>(null);
  const [detail, setDetail] = useState<PetDetailDto | null>(null);
  const [mappings, setMappings] = useState<StateMapping[]>([]);
  const [draftProc, setDraftProc] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    const c = await invoke<AppConfig>("get_app_config");
    setAppCfg(c);
    const sc = await invoke<StateConfig>("get_state_config");
    setMappings(sc.mappings);
    if (!c.activePetId) {
      setDetail(null);
      return;
    }
    try {
      const d = await invoke<PetDetailDto>("get_pet_detail", {
        folderId: c.activePetId,
      });
      setDetail(d);
    } catch {
      setDetail(null);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    const unlisten = listen<AppConfig>("app-config-changed", () => {
      void load();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [load]);

  const editableRows = useMemo(() => {
    if (!detail) return [] as EditableRow[];
    return buildEditableRows(detail.atlas.rows);
  }, [detail]);

  const persist = useCallback(async (next: StateMapping[]) => {
    setMappings(next);
    await invoke("save_state_config", { config: { mappings: next } });
  }, []);

  const upsertMapping = useCallback(
    async (state: string, trigger: TriggerConfig | null) => {
      const filtered = mappings.filter((m) => m.state !== state);
      if (!trigger) {
        await persist(filtered);
        return;
      }
      const err = validateMappingUniqueness(filtered, state, trigger);
      if (err) {
        setError(err);
        return;
      }
      setError(null);
      const next: StateMapping[] = [...filtered, { state, trigger }];
      await persist(next);
    },
    [mappings, persist],
  );

  const addProcess = useCallback(
    async (state: string, raw: string) => {
      const cur = mappingForState(mappings, state);
      if (!cur || cur.trigger.type !== "processFocus") return;
      const name = normalizeExe(raw);
      if (!name) return;
      if (cur.trigger.processes.some((x) => x.toLowerCase() === name)) return;
      await upsertMapping(state, {
        type: "processFocus",
        processes: [...cur.trigger.processes, name],
      });
    },
    [mappings, upsertMapping],
  );

  const removeProcess = useCallback(
    async (state: string, exe: string) => {
      const cur = mappingForState(mappings, state);
      if (!cur || cur.trigger.type !== "processFocus") return;
      const procs = cur.trigger.processes.filter(
        (x) => x.toLowerCase() !== exe.toLowerCase(),
      );
      await upsertMapping(state, { type: "processFocus", processes: procs });
    },
    [mappings, upsertMapping],
  );

  if (!appCfg?.activePetId) {
    return (
      <div>
        <h2 className="mb-4 text-xl font-bold">状态配置</h2>
        <p className="text-sm text-gray-500">
          请先在宠物库中设为当前宠物。
        </p>
      </div>
    );
  }

  if (!detail) {
    return (
      <div>
        <h2 className="mb-4 text-xl font-bold">状态配置</h2>
        <p className="text-sm text-gray-500">无法加载当前宠物资源。</p>
      </div>
    );
  }

  return (
    <div>
      <h2 className="mb-2 text-xl font-bold">状态配置</h2>
      <p className="mb-2 text-xs text-gray-500">
        当前宠物：{detail.manifest.displayName}。优先级：麦克风 &gt; 挂机 &gt;
        专注 &gt; 资源占用 &gt; 音频 &gt; 进程聚焦 &gt; 待机。
      </p>
      {error ? (
        <p className="mb-3 text-sm text-red-600">{error}</p>
      ) : null}
      <ul className="space-y-4">
        {editableRows.map((row) => {
          const mapping = mappingForState(mappings, row.state);
          const disabled =
            row.isDragBound || (row.isBase && !row.supported);
          const triggerType = mapping?.trigger.type ?? "";
          const draft = draftProc[row.state] ?? "";

          return (
            <li
              key={row.state}
              className={`flex flex-col gap-3 rounded-lg border p-3 ${
                disabled ? "bg-gray-50 opacity-60" : ""
              }`}
            >
              <div className="flex flex-wrap items-center gap-3">
                <div className="min-w-[120px] font-medium">
                  {row.label}
                  {row.isDragBound ? (
                    <span className="ml-2 text-xs text-gray-500">
                      （拖拽触发）
                    </span>
                  ) : null}
                  {row.isBase && !row.supported ? (
                    <span className="ml-2 text-xs text-amber-600">
                      （此宠物不支持）
                    </span>
                  ) : null}
                </div>
                <select
                  className="rounded border px-2 py-1 text-sm disabled:bg-gray-100"
                  disabled={disabled}
                  value={triggerType}
                  onChange={(e) => {
                    const v = e.target.value as TriggerType | "";
                    if (!v) {
                      void upsertMapping(row.state, null);
                      return;
                    }
                    void upsertMapping(row.state, defaultTrigger(v));
                  }}
                >
                  <option value="">不配置</option>
                  {Object.entries(TRIGGER_TYPE_LABELS).map(([k, label]) => (
                    <option key={k} value={k}>
                      {label}
                    </option>
                  ))}
                </select>
              </div>

              {!disabled && mapping ? (
                <TriggerEditor
                  mapping={mapping}
                  draftProc={draft}
                  onDraftChange={(v) =>
                    setDraftProc((d) => ({ ...d, [row.state]: v }))
                  }
                  onUpdate={(t) => void upsertMapping(row.state, t)}
                  onAddProcess={(raw) => void addProcess(row.state, raw)}
                  onRemoveProcess={(exe) => void removeProcess(row.state, exe)}
                />
              ) : null}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function TriggerEditor({
  mapping,
  draftProc,
  onDraftChange,
  onUpdate,
  onAddProcess,
  onRemoveProcess,
}: {
  mapping: StateMapping;
  draftProc: string;
  onDraftChange: (v: string) => void;
  onUpdate: (t: TriggerConfig) => void;
  onAddProcess: (raw: string) => void;
  onRemoveProcess: (exe: string) => void;
}) {
  const t = mapping.trigger;

  if (t.type === "processFocus") {
    return (
      <div className="flex flex-col gap-2 pl-2">
        <div className="flex flex-wrap gap-2">
          {t.processes.map((p) => (
            <span
              key={p}
              className="inline-flex items-center gap-1 rounded-full bg-pink-50 px-2 py-0.5 text-xs"
            >
              {p}
              <button
                type="button"
                className="text-pink-700"
                onClick={() => onRemoveProcess(p)}
              >
                ×
              </button>
            </span>
          ))}
        </div>
        <div className="flex flex-wrap gap-2">
          <select
            className="rounded border px-2 py-1 text-sm"
            defaultValue=""
            onChange={(e) => {
              if (!e.target.value) return;
              onAddProcess(e.target.value);
              e.target.value = "";
            }}
          >
            <option value="">添加预设进程…</option>
            {PRESET_PROCESSES.map((p) => (
              <option key={p.value} value={p.value}>
                {p.label}
              </option>
            ))}
          </select>
          <input
            className="rounded border px-2 py-1 text-sm"
            placeholder="或输入进程名"
            value={draftProc}
            onChange={(e) => onDraftChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key !== "Enter") return;
              onAddProcess(draftProc);
              onDraftChange("");
            }}
          />
          <button
            type="button"
            className="rounded bg-gray-900 px-3 py-1 text-sm text-white"
            onClick={() => {
              onAddProcess(draftProc);
              onDraftChange("");
            }}
          >
            添加
          </button>
        </div>
      </div>
    );
  }

  if (t.type === "highResource") {
    return (
      <div className="flex flex-wrap items-center gap-2 pl-2">
        <select
          className="rounded border px-2 py-1 text-sm"
          value={t.resource}
          onChange={(e) =>
            onUpdate({
              type: "highResource",
              resource: e.target.value as "cpu" | "memory",
              threshold: t.threshold,
            })
          }
        >
          <option value="cpu">CPU</option>
          <option value="memory">内存</option>
        </select>
        <label className="flex items-center gap-1 text-sm">
          阈值
          <input
            type="number"
            min={1}
            max={100}
            className="w-16 rounded border px-2 py-1"
            value={t.threshold}
            onChange={(e) =>
              onUpdate({
                type: "highResource",
                resource: t.resource,
                threshold: Number(e.target.value) || 90,
              })
            }
          />
          %
        </label>
      </div>
    );
  }

  if (t.type === "continuousFocus" || t.type === "computerIdle") {
    return (
      <div className="pl-2">
        <label className="flex items-center gap-2 text-sm">
          分钟
          <input
            type="number"
            min={1}
            max={999}
            className="w-20 rounded border px-2 py-1"
            value={t.minutes}
            onChange={(e) =>
              onUpdate({
                ...t,
                minutes: Math.max(1, Number(e.target.value) || 1),
              })
            }
          />
        </label>
      </div>
    );
  }

  return (
    <p className="pl-2 text-xs text-gray-500">
      系统将自动检测，无需额外配置。
    </p>
  );
}
