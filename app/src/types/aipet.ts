export interface AppConfig {
  alwaysOnTop: boolean;
  autoStart: boolean;
  animationSpeed: number;
  animationScale: number;
  activePetId: string;
}

export type TriggerConfig =
  | { type: "processFocus"; processes: string[] }
  | { type: "highResource"; resource: "cpu" | "memory"; threshold: number }
  | { type: "audioPlaying" }
  | { type: "microphoneActive" }
  | { type: "continuousFocus"; minutes: number }
  | { type: "computerIdle"; minutes: number };

export interface StateMapping {
  state: string;
  trigger: TriggerConfig;
}

export interface StateConfig {
  mappings: StateMapping[];
}

export interface SystemState {
  activeProcess: string;
  cpuPercent: number;
  memoryPercent: number;
  audioPlaying: boolean;
  microphoneActive: boolean;
  idleSeconds: number;
  focusSeconds: number;
}

export interface RunningProcessItem {
  exe: string;
  displayName: string;
}

export interface PetAtlasRow {
  state: string;
  displayName: string;
  frames: number;
}

export interface PetAtlas {
  cellWidth: number;
  cellHeight: number;
  rows: PetAtlasRow[];
}

export interface PetManifest {
  id: string;
  displayName: string;
  description: string;
  spritesheetPath: string;
}

export interface PetListItem {
  folderId: string;
  displayName: string;
  description: string;
  spritesheetPath: string;
}

export interface PetDetailDto {
  folderId: string;
  manifest: PetManifest;
  atlas: PetAtlas;
}

export type TriggerType = TriggerConfig["type"];
