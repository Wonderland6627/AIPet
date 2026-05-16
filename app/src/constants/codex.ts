/** 九个基础动作：界面固定展示；若当前宠物 atlas 无对应 state 则置灰不可编辑。 */
export const CODEX_BASE_STATES: { state: string; label: string }[] = [
  { state: "idle", label: "待机" },
  { state: "running-right", label: "向右跑" },
  { state: "running-left", label: "向左跑" },
  { state: "waving", label: "挥手" },
  { state: "jumping", label: "跳跃" },
  { state: "failed", label: "失败" },
  { state: "waiting", label: "等待" },
  { state: "running", label: "奔跑" },
  { state: "review", label: "审视" },
];

/** 与鼠标拖拽绑定，不可在状态配置中编辑。 */
export const DRAG_BOUND_STATES = new Set(["running-right", "running-left"]);

/** 与 `pet-atlas.json` 默认行一致（无主宠物资源时的兜底）。 */
export const DEFAULT_FALLBACK_ROWS = [
  { state: "idle", displayName: "待机", frames: 6 },
  { state: "running-right", displayName: "向右跑", frames: 8 },
  { state: "running-left", displayName: "向左跑", frames: 8 },
  { state: "waving", displayName: "挥手", frames: 4 },
  { state: "jumping", displayName: "跳跃", frames: 5 },
  { state: "failed", displayName: "失败", frames: 8 },
  { state: "waiting", displayName: "等待", frames: 6 },
  { state: "running", displayName: "奔跑", frames: 6 },
  { state: "review", displayName: "审视", frames: 6 },
] as const;

export const PRESET_PROCESSES = [
  { label: "Cursor", value: "cursor.exe" },
  { label: "VS Code", value: "code.exe" },
  { label: "Rider", value: "rider.exe" },
  { label: "Unity", value: "unity.exe" },
  { label: "Visual Studio", value: "devenv.exe" },
  { label: "Photoshop", value: "photoshop.exe" },
  { label: "Photoshop (ps)", value: "ps.exe" },
  { label: "After Effects", value: "afterfx.exe" },
  { label: "Excel", value: "excel.exe" },
  { label: "WPS", value: "wps.exe" },
  { label: "Blender", value: "blender.exe" },
  { label: "Figma", value: "figma.exe" },
];

export const TRIGGER_TYPE_LABELS: Record<string, string> = {
  processFocus: "当前聚焦进程",
  highResource: "CPU/内存占用过高",
  audioPlaying: "正在播放音频",
  microphoneActive: "麦克风被占用",
  continuousFocus: "持续专注",
  computerIdle: "电脑挂机",
};
