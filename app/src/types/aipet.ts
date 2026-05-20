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

// AI API Configuration
export interface AiProviderSettings {
  apiKey: string;
  baseUrl: string;
  model: string;
}

export interface AiApiConfig {
  provider: string;
  apiKey: string;
  baseUrl: string;
  model: string;
  providerCache?: Record<string, AiProviderSettings>;
}

// Pet Creation
export interface PetCreationRequest {
  petName: string;
  description: string;
  referenceImage: string | null;
  stylePreset: string;
}

export interface PetCreationProgress {
  taskId: string;
  step: number;
  subStep: string;
  status: "pending" | "running" | "completed" | "failed";
  message: string;
  error?: string;
}

export interface PetCreationResult {
  taskId: string;
  petId: string;
}

export interface PetCreationBaseReady {
  taskId: string;
  baseImageB64: string;
}

export interface IncompleteCreationTask {
  workDirName: string;
  petName: string;
  baseImageDone: boolean;
  completedCount: number;
  totalRows: number;
}
