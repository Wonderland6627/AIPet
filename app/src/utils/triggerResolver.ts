import type {
  PetAtlasRow,
  StateConfig,
  StateMapping,
  SystemState,
  TriggerConfig,
} from "../types/aipet";

const PRIORITY: TriggerConfig["type"][] = [
  "microphoneActive",
  "computerIdle",
  "continuousFocus",
  "highResource",
  "audioPlaying",
  "processFocus",
];

function mappingMatches(
  mapping: StateMapping,
  system: SystemState,
): boolean {
  const t = mapping.trigger;
  switch (t.type) {
    case "microphoneActive":
      return system.microphoneActive;
    case "computerIdle":
      return system.idleSeconds >= t.minutes * 60;
    case "continuousFocus":
      return system.focusSeconds >= t.minutes * 60;
    case "highResource": {
      const val =
        t.resource === "cpu" ? system.cpuPercent : system.memoryPercent;
      return val >= t.threshold;
    }
    case "audioPlaying":
      return system.audioPlaying;
    case "processFocus": {
      const key = system.activeProcess.toLowerCase();
      if (!key) return false;
      return t.processes.some((p) => p.toLowerCase() === key);
    }
    default:
      return false;
  }
}

export function resolveAnimationRowIndex(
  system: SystemState,
  stateConfig: StateConfig,
  atlasRows: PetAtlasRow[],
): number {
  for (const triggerType of PRIORITY) {
    for (const mapping of stateConfig.mappings) {
      if (mapping.trigger.type !== triggerType) continue;
      if (!mappingMatches(mapping, system)) continue;
      const idx = atlasRows.findIndex((r) => r.state === mapping.state);
      if (idx >= 0) return idx;
    }
  }

  const idleIdx = atlasRows.findIndex((r) => r.state === "idle");
  if (idleIdx >= 0) return idleIdx;
  return 0;
}

const HIGH_RESOURCES = ["cpu", "memory"] as const;
export type HighResourceKind = (typeof HIGH_RESOURCES)[number];

/** 已被其它状态占用的硬件资源（cpu / memory）。 */
export function getUsedResources(
  mappings: StateMapping[],
  excludeState: string,
): Set<HighResourceKind> {
  const used = new Set<HighResourceKind>();
  for (const m of mappings) {
    if (m.state === excludeState) continue;
    if (m.trigger.type !== "highResource") continue;
    used.add(m.trigger.resource);
  }
  return used;
}

/** 选择第一个未被占用的资源；若均已占用则返回 null。 */
export function pickAvailableResource(
  mappings: StateMapping[],
  excludeState: string,
): HighResourceKind | null {
  const used = getUsedResources(mappings, excludeState);
  for (const r of HIGH_RESOURCES) {
    if (!used.has(r)) return r;
  }
  return null;
}

export interface ResolvedPetStatus {
  state: string;
  displayName: string;
  reason: string;
  isIdle: boolean;
}

function findMatchingMapping(
  system: SystemState,
  stateConfig: StateConfig,
): StateMapping | null {
  for (const triggerType of PRIORITY) {
    for (const mapping of stateConfig.mappings) {
      if (mapping.trigger.type !== triggerType) continue;
      if (!mappingMatches(mapping, system)) continue;
      return mapping;
    }
  }
  return null;
}

export function resolvePetStatus(
  system: SystemState,
  stateConfig: StateConfig,
  atlasRows: PetAtlasRow[],
): ResolvedPetStatus {
  const mapping = findMatchingMapping(system, stateConfig);
  if (!mapping) {
    const idleRow = atlasRows.find((r) => r.state === "idle");
    return {
      state: "idle",
      displayName: idleRow?.displayName ?? "待机",
      reason: "无匹配触发条件",
      isIdle: true,
    };
  }

  const row = atlasRows.find((r) => r.state === mapping.state);
  const displayName = row?.displayName ?? mapping.state;
  const reason = describeTriggerReason(mapping, system);

  return {
    state: mapping.state,
    displayName,
    reason,
    isIdle: mapping.state === "idle",
  };
}

function describeTriggerReason(
  mapping: StateMapping,
  system: SystemState,
): string {
  const t = mapping.trigger;
  switch (t.type) {
    case "processFocus":
      return `进程聚焦: ${system.activeProcess || "未知"}`;
    case "highResource": {
      const val =
        t.resource === "cpu" ? system.cpuPercent : system.memoryPercent;
      const label = t.resource === "cpu" ? "CPU" : "内存";
      return `${label} 占用 ${Math.round(val)}%（阈值 ${t.threshold}%）`;
    }
    case "audioPlaying":
      return "正在播放音频";
    case "microphoneActive":
      return "麦克风被占用";
    case "continuousFocus":
      return `持续专注 ${Math.floor(system.focusSeconds / 60)} 分钟（需 ${t.minutes} 分钟）`;
    case "computerIdle":
      return `电脑挂机 ${Math.floor(system.idleSeconds / 60)} 分钟（需 ${t.minutes} 分钟）`;
    default:
      return "系统触发";
  }
}

export function normalizeExe(raw: string): string {
  const v = raw.trim().toLowerCase();
  if (!v) return "";
  if (v.endsWith(".exe")) return v;
  return `${v}.exe`;
}

/** 校验唯一性：返回错误信息，空字符串表示通过。 */
export function validateMappingUniqueness(
  mappings: StateMapping[],
  editingState: string,
  trigger: TriggerConfig,
): string {
  for (const m of mappings) {
    if (m.state === editingState) continue;

    if (trigger.type === "processFocus" && m.trigger.type === "processFocus") {
      for (const p of trigger.processes) {
        if (m.trigger.processes.some((x) => x.toLowerCase() === p.toLowerCase())) {
          return `进程 ${p} 已被「${m.state}」使用`;
        }
      }
    }

    if (
      trigger.type === "highResource" &&
      m.trigger.type === "highResource" &&
      trigger.resource === m.trigger.resource
    ) {
      return `${trigger.resource.toUpperCase()} 阈值已被「${m.state}」使用`;
    }

    if (
      trigger.type !== "processFocus" &&
      trigger.type !== "highResource" &&
      m.trigger.type === trigger.type
    ) {
      return `触发类型已被「${m.state}」使用`;
    }
  }
  return "";
}
