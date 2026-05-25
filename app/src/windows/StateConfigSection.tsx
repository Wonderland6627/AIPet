import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  DRAG_BOUND_STATES,
  TRIGGER_TYPE_LABELS,
} from "../constants/codex";
import type {
  AppConfig,
  PetAtlasRow,
  PetDetailDto,
  RunningProcessItem,
  StateConfig,
  StateMapping,
  TriggerConfig,
  TriggerType,
} from "../types/aipet";
import {
  getUsedResources,
  normalizeExe,
  pickAvailableResource,
  validateMappingUniqueness,
} from "../utils/triggerResolver";

const UNIQUE_TRIGGER_TYPES: TriggerType[] = [
  "audioPlaying",
  "microphoneActive",
  "computerIdle",
  "continuousFocus",
];

const ALL_TRIGGER_TYPES = Object.keys(TRIGGER_TYPE_LABELS) as TriggerType[];

type AnimationOption = {
  state: string;
  label: string;
};

function selectableAnimations(rows: PetAtlasRow[]): AnimationOption[] {
  return rows
    .filter((r) => !DRAG_BOUND_STATES.has(r.state) && r.state !== "idle")
    .map((r) => ({ state: r.state, label: r.displayName }));
}

function defaultTrigger(type: TriggerType): TriggerConfig {
  switch (type) {
    case "processFocus":
      return { type: "processFocus", processes: [] };
    case "highResource":
      return { type: "highResource", resource: "cpu", threshold: 60 };
    case "audioPlaying":
      return { type: "audioPlaying" };
    case "microphoneActive":
      return { type: "microphoneActive" };
    case "continuousFocus":
      return { type: "continuousFocus", minutes: 20 };
    case "computerIdle":
      return { type: "computerIdle", minutes: 10 };
    default:
      return { type: "processFocus", processes: [] };
  }
}

function getUsedUniqueTriggers(
  mappings: StateMapping[],
  excludeState: string,
): Set<TriggerType> {
  const used = new Set<TriggerType>();
  for (const m of mappings) {
    if (m.state === excludeState) continue;
    if (UNIQUE_TRIGGER_TYPES.includes(m.trigger.type)) {
      used.add(m.trigger.type);
    }
  }
  return used;
}

function getAvailableTriggerTypes(
  mappings: StateMapping[],
  excludeState: string,
): TriggerType[] {
  const usedUnique = getUsedUniqueTriggers(mappings, excludeState);
  const usedResources = getUsedResources(mappings, excludeState);

  return ALL_TRIGGER_TYPES.filter((t) => {
    if (UNIQUE_TRIGGER_TYPES.includes(t)) {
      return !usedUnique.has(t);
    }
    if (t === "highResource") {
      return usedResources.size < 2;
    }
    return true;
  });
}

function getUsedAnimationStates(
  mappings: StateMapping[],
  excludeState: string,
): Set<string> {
  const used = new Set<string>();
  for (const m of mappings) {
    if (m.state === excludeState) continue;
    used.add(m.state);
  }
  return used;
}

function getAvailableAnimations(
  rows: PetAtlasRow[],
  mappings: StateMapping[],
  excludeState: string,
): AnimationOption[] {
  const used = getUsedAnimationStates(mappings, excludeState);
  return selectableAnimations(rows).filter((a) => !used.has(a.state));
}

function getUsedProcessesGlobally(
  mappings: StateMapping[],
  excludeState: string,
): Set<string> {
  const used = new Set<string>();
  for (const m of mappings) {
    if (m.state === excludeState) continue;
    if (m.trigger.type !== "processFocus") continue;
    for (const p of m.trigger.processes) {
      used.add(p.toLowerCase());
    }
  }
  return used;
}

function pickDefaultTrigger(mappings: StateMapping[]): TriggerConfig | null {
  const types = getAvailableTriggerTypes(mappings, "");
  if (types.length === 0) return null;

  const type = types[0];
  if (type === "highResource") {
    const resource = pickAvailableResource(mappings, "");
    if (!resource) return null;
    return { type: "highResource", resource, threshold: 60 };
  }
  return defaultTrigger(type);
}

function animationLabel(rows: PetAtlasRow[], state: string): string {
  return rows.find((r) => r.state === state)?.displayName ?? state;
}

export function StateConfigSection() {
  const [appCfg, setAppCfg] = useState<AppConfig | null>(null);
  const [detail, setDetail] = useState<PetDetailDto | null>(null);
  const [mappings, setMappings] = useState<StateMapping[]>([]);
  const [draftProc, setDraftProc] = useState<Record<string, string>>({});
  const [runningProcesses, setRunningProcesses] = useState<RunningProcessItem[]>([]);
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

  const loadRunningProcesses = useCallback(async () => {
    try {
      const list = await invoke<RunningProcessItem[]>("list_running_processes");
      setRunningProcesses(list);
    } catch {
      setRunningProcesses([]);
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

  const hasProcessFocusMapping = useMemo(
    () => mappings.some((m) => m.trigger.type === "processFocus"),
    [mappings],
  );

  useEffect(() => {
    if (!hasProcessFocusMapping) return;
    void loadRunningProcesses();
  }, [hasProcessFocusMapping, loadRunningProcesses]);

  const atlasRows = detail?.atlas.rows ?? [];

  const persist = useCallback(async (next: StateMapping[]) => {
    setMappings(next);
    await invoke("save_state_config", { config: { mappings: next } });
  }, []);

  const upsertMapping = useCallback(
    async (state: string, trigger: TriggerConfig) => {
      const index = mappings.findIndex((m) => m.state === state);
      const filtered =
        index >= 0 ? mappings.filter((_, i) => i !== index) : mappings;
      const err = validateMappingUniqueness(filtered, state, trigger);
      if (err) {
        setError(err);
        return;
      }
      setError(null);
      if (index < 0) {
        await persist([...mappings, { state, trigger }]);
        return;
      }
      const next: StateMapping[] = [...mappings];
      next[index] = { state, trigger };
      await persist(next);
    },
    [mappings, persist],
  );

  const removeMapping = useCallback(
    async (state: string) => {
      setError(null);
      await persist(mappings.filter((m) => m.state !== state));
    },
    [mappings, persist],
  );

  const changeAnimation = useCallback(
    async (oldState: string, newState: string) => {
      if (oldState === newState) return;
      const oldIndex = mappings.findIndex((m) => m.state === oldState);
      if (oldIndex < 0) return;
      const cur = mappings[oldIndex];
      const withoutOld = mappings.filter((_, i) => i !== oldIndex);
      const err = validateMappingUniqueness(withoutOld, newState, cur.trigger);
      if (err) {
        setError(err);
        return;
      }
      setError(null);
      const deduped = withoutOld.filter((m) => m.state !== newState);
      const insertAt = Math.min(oldIndex, deduped.length);
      const next = [...deduped];
      next.splice(insertAt, 0, { state: newState, trigger: cur.trigger });
      await persist(next);
    },
    [mappings, persist],
  );

  const changeTriggerType = useCallback(
    async (state: string, type: TriggerType) => {
      if (type === "highResource") {
        const resource = pickAvailableResource(mappings, state);
        if (!resource) {
          setError("CPU 与内存阈值均已绑定到其它条件");
          return;
        }
        await upsertMapping(state, {
          type: "highResource",
          resource,
          threshold: 60,
        });
        return;
      }
      await upsertMapping(state, defaultTrigger(type));
    },
    [mappings, upsertMapping],
  );

  const addProcess = useCallback(
    async (state: string, raw: string) => {
      const cur = mappings.find((m) => m.state === state);
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
      const cur = mappings.find((m) => m.state === state);
      if (!cur || cur.trigger.type !== "processFocus") return;
      const procs = cur.trigger.processes.filter(
        (x) => x.toLowerCase() !== exe.toLowerCase(),
      );
      await upsertMapping(state, { type: "processFocus", processes: procs });
    },
    [mappings, upsertMapping],
  );

  const handleCreate = useCallback(async () => {
    if (!detail) return;
    const anim = getAvailableAnimations(atlasRows, mappings, "")[0];
    if (!anim) {
      setError("没有可分配的动画");
      return;
    }
    const trigger = pickDefaultTrigger(mappings);
    if (!trigger) {
      setError("没有可分配的触发条件");
      return;
    }
    const err = validateMappingUniqueness(mappings, anim.state, trigger);
    if (err) {
      setError(err);
      return;
    }
    setError(null);
    await persist([...mappings, { state: anim.state, trigger }]);
  }, [atlasRows, detail, mappings, persist]);

  const canCreate = useMemo(() => {
    if (!detail) return false;
    return (
      getAvailableAnimations(atlasRows, mappings, "").length > 0 &&
      getAvailableTriggerTypes(mappings, "").length > 0
    );
  }, [atlasRows, detail, mappings]);

  const createDisabledReason = useMemo(() => {
    if (!detail) return "";
    const noAnim = getAvailableAnimations(atlasRows, mappings, "").length === 0;
    const noTrigger = getAvailableTriggerTypes(mappings, "").length === 0;
    if (noAnim && noTrigger) return "（可用动画与触发条件均已分配完毕）";
    if (noAnim) return "（可用动画已全部分配）";
    if (noTrigger) return "（可用触发条件已全部分配）";
    return "";
  }, [atlasRows, detail, mappings]);

  if (!appCfg?.activePetId) {
    return (
      <div className="flex min-h-full flex-col gap-4">
        <h2 className="text-xl font-bold">状态配置</h2>
        <section className="rounded-xl border border-gray-100 bg-white p-4 shadow-sm">
          <p className="text-sm text-gray-500">请先在宠物库中设为当前宠物。</p>
        </section>
      </div>
    );
  }

  if (!detail) {
    return (
      <div className="flex min-h-full flex-col gap-4">
        <h2 className="text-xl font-bold">状态配置</h2>
        <section className="rounded-xl border border-gray-100 bg-white p-4 shadow-sm">
          <p className="text-sm text-gray-500">无法加载当前宠物资源。</p>
        </section>
      </div>
    );
  }

  return (
    <div className="flex min-h-full flex-col gap-4">
      <h2 className="text-xl font-bold">状态配置</h2>
      <section className="rounded-xl border border-gray-100 bg-white p-4 shadow-sm">
        <p className="mb-3 text-xs text-gray-500">
          当前宠物：{detail.manifest.displayName}。优先级：麦克风 &gt; 挂机 &gt;
          专注 &gt; 资源占用 &gt; 音频 &gt; 进程聚焦 &gt; 待机。向左跑、向右跑由拖拽触发。
        </p>
        {error ? (
          <p className="mb-3 rounded-lg bg-red-50 px-3 py-2 text-xs text-red-600">{error}</p>
        ) : null}

        <ul className="space-y-3">
        {mappings.map((mapping) => {
          const triggerOptions = getAvailableTriggerTypes(
            mappings,
            mapping.state,
          );
          const currentType = mapping.trigger.type;
          const triggerSelectOptions = triggerOptions.includes(currentType)
            ? triggerOptions
            : [currentType, ...triggerOptions];

          const animOptions = getAvailableAnimations(
            atlasRows,
            mappings,
            mapping.state,
          );
          const currentAnim = mapping.state;
          const animSelectOptions = animOptions.some((a) => a.state === currentAnim)
            ? animOptions
            : [
                { state: currentAnim, label: animationLabel(atlasRows, currentAnim) },
                ...animOptions,
              ];

          const draft = draftProc[mapping.state] ?? "";
          const globalUsedProcs = getUsedProcessesGlobally(
            mappings,
            mapping.state,
          );

          return (
            <li
              key={mapping.state}
              className="flex flex-wrap items-start gap-2 rounded-xl border border-gray-100 bg-gray-50 p-3"
            >
              <select
                className="w-[140px] shrink-0 rounded-lg border border-gray-200 bg-white px-2.5 py-1.5 text-sm text-gray-700"
                value={currentType}
                onChange={(e) => {
                  void changeTriggerType(
                    mapping.state,
                    e.target.value as TriggerType,
                  );
                }}
              >
                {triggerSelectOptions.map((t) => (
                  <option key={t} value={t}>
                    {TRIGGER_TYPE_LABELS[t]}
                  </option>
                ))}
              </select>

              <div className="min-w-[160px] flex-1">
                <TriggerParams
                  mapping={mapping}
                  usedResources={getUsedResources(mappings, mapping.state)}
                  draftProc={draft}
                  runningProcesses={runningProcesses}
                  globalUsedProcs={globalUsedProcs}
                  onDraftChange={(v) =>
                    setDraftProc((d) => ({ ...d, [mapping.state]: v }))
                  }
                  onUpdate={(t) => void upsertMapping(mapping.state, t)}
                  onAddProcess={(raw) => void addProcess(mapping.state, raw)}
                  onRemoveProcess={(exe) =>
                    void removeProcess(mapping.state, exe)
                  }
                  onLoadRunningProcesses={() => void loadRunningProcesses()}
                />
              </div>

              <select
                className="w-[120px] shrink-0 rounded-lg border border-gray-200 bg-white px-2.5 py-1.5 text-sm text-gray-700"
                value={currentAnim}
                onChange={(e) => {
                  void changeAnimation(mapping.state, e.target.value);
                }}
              >
                {animSelectOptions.map((a) => (
                  <option key={a.state} value={a.state}>
                    {a.label}
                  </option>
                ))}
              </select>

              <button
                type="button"
                className="shrink-0 rounded-lg border border-red-200 px-2.5 py-1.5 text-sm text-red-500 hover:bg-red-50"
                title="移除此条件"
                onClick={() => void removeMapping(mapping.state)}
              >
                移除
              </button>
            </li>
          );
        })}
        </ul>

        <button
          type="button"
          className="mt-4 flex w-full items-center justify-center gap-2 rounded-xl border-2 border-dashed border-pink-300 bg-pink-50/50 px-4 py-3 text-sm font-medium text-pink-600 transition-colors hover:border-pink-400 hover:bg-pink-100/60 disabled:cursor-not-allowed disabled:opacity-50"
          disabled={!canCreate}
          onClick={() => void handleCreate()}
        >
          <span className="text-lg leading-none">+</span>
          <span>创建条件{!canCreate ? ` ${createDisabledReason}` : ""}</span>
        </button>
      </section>
    </div>
  );
}

function TriggerParams({
  mapping,
  usedResources,
  draftProc,
  runningProcesses,
  globalUsedProcs,
  onDraftChange,
  onUpdate,
  onAddProcess,
  onRemoveProcess,
  onLoadRunningProcesses,
}: {
  mapping: StateMapping;
  usedResources: Set<"cpu" | "memory">;
  draftProc: string;
  runningProcesses: RunningProcessItem[];
  globalUsedProcs: Set<string>;
  onDraftChange: (v: string) => void;
  onUpdate: (t: TriggerConfig) => void;
  onAddProcess: (raw: string) => void;
  onRemoveProcess: (exe: string) => void;
  onLoadRunningProcesses: () => void;
}) {
  const t = mapping.trigger;
  const localUsed = new Set(
    t.type === "processFocus"
      ? t.processes.map((p) => p.toLowerCase())
      : [],
  );

  if (t.type === "processFocus") {
    const selectableRunning = runningProcesses.filter(
      (p) => !globalUsedProcs.has(p.exe.toLowerCase()) && !localUsed.has(p.exe.toLowerCase()),
    );

    return (
      <div className="flex flex-col gap-2">
        {t.processes.length > 0 ? (
          <div className="flex flex-wrap gap-1">
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
        ) : null}
        <div className="flex flex-wrap items-center gap-2">
          <select
            className="min-w-[260px] max-w-full rounded-lg border border-gray-200 bg-white px-2.5 py-1.5 text-sm text-gray-700"
            defaultValue=""
            onMouseDown={onLoadRunningProcesses}
            onFocus={onLoadRunningProcesses}
            onChange={(e) => {
              if (!e.target.value) return;
              onAddProcess(e.target.value);
              e.target.value = "";
            }}
          >
            <option value="">添加正在运行的进程…</option>
            {selectableRunning.map((p) => (
              <option key={p.exe} value={p.exe}>
                {`${p.displayName}（${p.exe}）`}
              </option>
            ))}
          </select>
          <input
            className="min-w-[120px] flex-1 rounded-lg border border-gray-200 bg-white px-2.5 py-1.5 text-sm text-gray-700"
            placeholder="或输入进程名(.exe)"
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
            className="rounded-lg bg-pink-500 px-3 py-1.5 text-sm font-medium text-white hover:bg-pink-600"
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
      <div className="flex min-h-8 flex-wrap items-center gap-2">
        <select
          className="rounded-lg border border-gray-200 bg-white px-2.5 py-1.5 text-sm text-gray-700"
          value={t.resource}
          onChange={(e) =>
            onUpdate({
              type: "highResource",
              resource: e.target.value as "cpu" | "memory",
              threshold: t.threshold,
            })
          }
        >
          <option value="cpu" disabled={usedResources.has("cpu")}>
            CPU{usedResources.has("cpu") ? "（已占用）" : ""}
          </option>
          <option value="memory" disabled={usedResources.has("memory")}>
            内存{usedResources.has("memory") ? "（已占用）" : ""}
          </option>
        </select>
        <label className="flex items-center gap-1 text-sm">
          阈值
          <input
            type="number"
            min={1}
            max={100}
            className="w-16 rounded-lg border border-gray-200 bg-white px-2 py-1.5"
            value={t.threshold}
            onChange={(e) =>
              onUpdate({
                type: "highResource",
                resource: t.resource,
                threshold: Number(e.target.value) || 60,
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
      <label className="flex items-center gap-2 text-sm">
        分钟
        <input
          type="number"
          min={1}
          max={999}
          className="w-20 rounded-lg border border-gray-200 bg-white px-2 py-1.5"
          value={t.minutes}
          onChange={(e) =>
            onUpdate({
              ...t,
              minutes: Math.max(1, Number(e.target.value) || 1),
            })
          }
        />
      </label>
    );
  }

  return (
    <p className="py-1.5 text-xs text-gray-500">系统将自动检测，无需额外配置。</p>
  );
}
