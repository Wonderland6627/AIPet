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
