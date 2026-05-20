import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  PetCreationProgress,
  PetCreationRequest,
  PetCreationResult,
  PetCreationBaseReady,
  IncompleteCreationTask,
} from "../types/aipet";

interface PetCreatorPanelProps {
  onClose: () => void;
  onCreated: () => void;
}

type Phase = "input" | "generating" | "base-confirm" | "completed" | "failed";

interface StepState {
  step: number;
  label: string;
  status: "pending" | "running" | "completed" | "failed";
  subSteps?: { name: string; status: string }[];
}

const STYLE_PRESETS = [
  { id: "3d-toy", label: "3D玩具" },
  { id: "pixel", label: "像素" },
  { id: "clay", label: "粘土" },
  { id: "sticker", label: "贴纸" },
  { id: "plush", label: "毛绒" },
  { id: "flat-vector", label: "扁平" },
];

const ANIMATION_STATES = [
  "idle",
  "running-right",
  "running-left",
  "waving",
  "jumping",
  "failed",
  "waiting",
  "running",
  "review",
];

const INITIAL_STEPS: StepState[] = [
  { step: 1, label: "准备工作目录", status: "pending" },
  { step: 2, label: "生成基础形象", status: "pending" },
  {
    step: 3,
    label: "生成动画帧",
    status: "pending",
    subSteps: ANIMATION_STATES.map((s) => ({ name: s, status: "pending" })),
  },
  { step: 4, label: "处理图像", status: "pending" },
  { step: 5, label: "检查帧质量", status: "pending" },
  { step: 6, label: "合成精灵图集", status: "pending" },
  { step: 7, label: "打包宠物文件", status: "pending" },
];

interface LogEntry {
  time: string;
  level: string;
  message: string;
}

export function PetCreatorPanel({ onClose, onCreated }: PetCreatorPanelProps) {
  const [phase, setPhase] = useState<Phase>("input");
  const [petName, setPetName] = useState("");
  const [description, setDescription] = useState("");
  const [stylePreset, setStylePreset] = useState("3d-toy");
  const [referenceImage, setReferenceImage] = useState<string | null>(null);
  const [referencePreview, setReferencePreview] = useState<string | null>(null);
  const [taskId, setTaskId] = useState<string | null>(null);
  const [steps, setSteps] = useState<StepState[]>(INITIAL_STEPS);
  const [errorMessage, setErrorMessage] = useState("");
  const [baseImageB64, setBaseImageB64] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [showLogs, setShowLogs] = useState(false);
  const [incompleteTasks, setIncompleteTasks] = useState<IncompleteCreationTask[]>([]);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const logEndRef = useRef<HTMLDivElement>(null);

  const canSubmit = (petName.trim() && (description.trim() || referenceImage)) && !submitting;

  const loadResumeTasks = useCallback(async () => {
    try {
      const tasks = await invoke<IncompleteCreationTask[]>("list_incomplete_tasks");
      setIncompleteTasks(tasks);
    } catch {
      setIncompleteTasks([]);
    }
  }, []);

  useEffect(() => {
    void loadResumeTasks();
  }, [loadResumeTasks]);

  // Listen for progress events
  useEffect(() => {
    const unlistenProgress = listen<PetCreationProgress>(
      "pet-creation-progress",
      (e) => {
        const p = e.payload;
        if (taskId && p.taskId !== taskId) return;

        setSteps((prev) =>
          prev.map((s) => {
            if (s.step === p.step) {
              const updated = { ...s, status: p.status as StepState["status"] };
              if (s.subSteps && p.subStep) {
                updated.subSteps = s.subSteps.map((sub) =>
                  sub.name === p.subStep
                    ? { ...sub, status: p.status }
                    : sub,
                );
              }
              return updated;
            }
            if (s.step < p.step && s.status !== "completed") {
              return { ...s, status: "completed" };
            }
            return s;
          }),
        );
      },
    );

    const unlistenCompleted = listen<PetCreationResult>(
      "pet-creation-completed",
      (e) => {
        if (taskId && e.payload.taskId !== taskId) return;
        setPhase("completed");
        setSteps((prev) => prev.map((s) => ({ ...s, status: "completed" })));
        // Auto-summon the newly created pet
        void (async () => {
          try {
            const cfg = await invoke<Record<string, unknown>>("get_app_config");
            await invoke("save_app_config", {
              config: { ...cfg, activePetId: e.payload.petId },
            });
          } catch { /* ignore */ }
        })();
      },
    );

    const unlistenFailed = listen<{ taskId: string; reason: string }>(
      "pet-creation-failed",
      (e) => {
        if (taskId && e.payload.taskId !== taskId) return;
        setPhase("failed");
        setErrorMessage(e.payload.reason);
      },
    );

    const unlistenCancelled = listen<{ taskId: string; reason: string }>(
      "pet-creation-cancelled",
      (e) => {
        if (taskId && e.payload.taskId !== taskId) return;
        handleReset();
      },
    );

    const unlistenBaseReady = listen<PetCreationBaseReady>(
      "pet-creation-base-ready",
      (e) => {
        if (taskId && e.payload.taskId !== taskId) return;
        setBaseImageB64(e.payload.baseImageB64);
        setPhase("base-confirm");
      },
    );

    const unlistenLog = listen<{ taskId: string; time: string; level: string; message: string }>(
      "pet-creation-log",
      (e) => {
        if (taskId && e.payload.taskId !== taskId) return;
        setLogs((prev) => [...prev, { time: e.payload.time, level: e.payload.level, message: e.payload.message }]);
      },
    );

    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenCompleted.then((fn) => fn());
      unlistenFailed.then((fn) => fn());
      unlistenCancelled.then((fn) => fn());
      unlistenBaseReady.then((fn) => fn());
      unlistenLog.then((fn) => fn());
    };
  }, [taskId]);

  useEffect(() => {
    if (showLogs && logEndRef.current) {
      logEndRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [logs, showLogs]);

  const handleReset = useCallback(() => {
    setPhase("input");
    setTaskId(null);
    setSteps(INITIAL_STEPS);
    setErrorMessage("");
    setBaseImageB64(null);
    setSubmitting(false);
    setLogs([]);
    setShowLogs(false);
    void loadResumeTasks();
  }, [loadResumeTasks]);

  const handleSubmit = useCallback(async () => {
    if (!canSubmit) return;
    setSubmitting(true);
    setErrorMessage("");

    try {
      const request: PetCreationRequest = {
        petName: petName.trim(),
        description: description.trim(),
        referenceImage: referenceImage,
        stylePreset,
      };

      const id = await invoke<string>("start_pet_creation", { request });
      setTaskId(id);
      setPhase("generating");
    } catch (e) {
      setErrorMessage(String(e));
      setSubmitting(false);
    }
  }, [canSubmit, petName, description, referenceImage, stylePreset]);

  const handleResume = useCallback(
    async (workDirName: string) => {
      if (submitting) return;
      setSubmitting(true);
      setErrorMessage("");
      setBaseImageB64(null);
      setSteps(INITIAL_STEPS);
      setLogs([]);
      try {
        const id = await invoke<string>("resume_pet_creation", { workDirName });
        setTaskId(id);
        setPhase("generating");
      } catch (e) {
        setErrorMessage(String(e));
        setSubmitting(false);
        void loadResumeTasks();
      }
    },
    [submitting, loadResumeTasks],
  );

  const handleCancel = useCallback(async () => {
    if (!taskId) {
      onClose();
      return;
    }
    try {
      await invoke("cancel_pet_creation", { taskId });
    } catch {
      // ignore
    }
    handleReset();
  }, [taskId, onClose, handleReset]);

  const handleConfirmBase = useCallback(
    async (confirmed: boolean) => {
      if (!taskId) return;
      if (!confirmed) {
        await invoke("confirm_base_image", { taskId, confirmed: false });
        handleReset();
        return;
      }
      await invoke("confirm_base_image", { taskId, confirmed: true });
      setPhase("generating");
    },
    [taskId, handleReset],
  );

  const [dragOver, setDragOver] = useState(false);
  const [imageError, setImageError] = useState("");

  const MAX_FILE_SIZE = 20 * 1024 * 1024; // 20MB
  const MAX_DIMENSION = 4096;
  const ACCEPTED_TYPES = ["image/png", "image/jpeg", "image/webp"];

  const validateAndProcessFile = useCallback((file: File) => {
    setImageError("");

    if (!ACCEPTED_TYPES.includes(file.type)) {
      setImageError("不支持的格式，请使用 PNG、JPG 或 WebP");
      return;
    }

    if (file.size > MAX_FILE_SIZE) {
      setImageError(`文件过大（${(file.size / 1024 / 1024).toFixed(1)}MB），最大支持 20MB`);
      return;
    }

    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      const img = new Image();
      img.onload = () => {
        if (img.width > MAX_DIMENSION || img.height > MAX_DIMENSION) {
          setImageError(`图片尺寸过大（${img.width}x${img.height}），单边最大 ${MAX_DIMENSION}px`);
          return;
        }
        if (img.width < 64 || img.height < 64) {
          setImageError(`图片尺寸过小（${img.width}x${img.height}），最小 64x64px`);
          return;
        }
        const ratio = Math.max(img.width, img.height) / Math.min(img.width, img.height);
        const b64 = result.split(",")[1];
        setReferenceImage(b64);
        setReferencePreview(result);
        if (ratio > 2) {
          setImageError(`提示：图片宽高比较大（${img.width}x${img.height}），接近正方形的图片效果更好`);
        } else {
          setImageError("");
        }
      };
      img.onerror = () => {
        setImageError("图片加载失败，请检查文件是否损坏");
      };
      img.src = result;
    };
    reader.readAsDataURL(file);
  }, []);

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragOver(false);
    const file = e.dataTransfer.files[0];
    if (!file) return;
    validateAndProcessFile(file);
  }, [validateAndProcessFile]);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragOver(true);
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragOver(false);
  }, []);

  const handleFileSelect = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    validateAndProcessFile(file);
  }, [validateAndProcessFile]);

  const removeReference = () => {
    setReferenceImage(null);
    setReferencePreview(null);
    setImageError("");
    if (fileInputRef.current) fileInputRef.current.value = "";
  };

  // Input phase
  if (phase === "input") {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
        <div className="w-[480px] rounded-2xl bg-white p-6 shadow-2xl">
          <h2 className="mb-4 text-center text-lg font-bold">捏一个新宠物</h2>

          <div className="mb-4">
            <label className="mb-1 block text-sm font-medium text-gray-700">
              宠物名称
            </label>
            <input
              type="text"
              className="w-full rounded-lg border border-gray-300 px-3 py-2 text-sm focus:border-pink-400 focus:outline-none"
              placeholder="给宠物取个名字"
              value={petName}
              onChange={(e) => setPetName(e.target.value)}
            />
          </div>

          <div className="mb-4">
            <label className="mb-1 block text-sm font-medium text-gray-700">
              描述你想要的宠物
            </label>
            <textarea
              className="h-24 w-full resize-none rounded-lg border border-gray-300 px-3 py-2 text-sm focus:border-pink-400 focus:outline-none"
              placeholder="描述宠物的外观、风格、特征...&#10;例如：一只穿着太空服的橘色小猫，戴着星形头盔，圆润可爱"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
          </div>

          <div className="mb-4">
            <label className="mb-2 block text-sm font-medium text-gray-700">
              参考图 (可选)
            </label>
            {referencePreview ? (
              <div className="relative inline-block">
                <img
                  src={referencePreview}
                  alt="参考图"
                  className="h-32 w-32 rounded-lg border border-gray-200 object-cover"
                />
                <button
                  type="button"
                  className="absolute -right-2 -top-2 flex h-5 w-5 items-center justify-center rounded-full bg-red-500 text-xs text-white hover:bg-red-600"
                  onClick={removeReference}
                >
                  ×
                </button>
              </div>
            ) : (
              <div
                className={`flex h-32 cursor-pointer flex-col items-center justify-center rounded-lg border-2 border-dashed transition-colors ${
                  dragOver
                    ? "border-pink-500 bg-pink-50"
                    : "border-gray-300 hover:border-pink-300 hover:bg-pink-50/30"
                }`}
                onDrop={handleDrop}
                onDragOver={handleDragOver}
                onDragEnter={handleDragOver}
                onDragLeave={handleDragLeave}
                onClick={() => fileInputRef.current?.click()}
              >
                <span className="mb-1 text-2xl text-gray-400">+</span>
                <span className="text-xs text-gray-400">
                  拖拽图片至此，或点击选择
                </span>
                <span className="mt-1 text-[10px] text-gray-300">
                  PNG / JPG / WebP，≤20MB，推荐接近正方形的图片
                </span>
              </div>
            )}
            {imageError && (
              <p className="mt-1 text-xs text-red-500">{imageError}</p>
            )}
            <input
              ref={fileInputRef}
              type="file"
              accept="image/png,image/jpeg,image/webp"
              className="hidden"
              onChange={handleFileSelect}
            />
          </div>

          <div className="mb-5">
            <label className="mb-2 block text-sm font-medium text-gray-700">
              风格
            </label>
            <div className="flex flex-wrap gap-2">
              {STYLE_PRESETS.map((s) => (
                <button
                  key={s.id}
                  type="button"
                  className={`rounded-full border px-3 py-1 text-xs transition-colors ${
                    stylePreset === s.id
                      ? "border-pink-500 bg-pink-50 text-pink-700"
                      : "border-gray-200 text-gray-600 hover:border-pink-200"
                  }`}
                  onClick={() => setStylePreset(s.id)}
                >
                  {s.label}
                </button>
              ))}
            </div>
          </div>

          {incompleteTasks.length > 0 && (
            <ResumeTasksCollapsible
              tasks={incompleteTasks}
              submitting={submitting}
              onResume={handleResume}
              onRefresh={loadResumeTasks}
            />
          )}

          {errorMessage && (
            <p className="mb-3 rounded-lg bg-red-50 px-3 py-2 text-xs text-red-600">
              {errorMessage}
            </p>
          )}

          <div className="flex justify-end gap-3">
            <button
              type="button"
              className="rounded-lg border border-gray-300 px-4 py-2 text-sm hover:bg-gray-50"
              onClick={onClose}
            >
              取消
            </button>
            <button
              type="button"
              disabled={!canSubmit}
              className="rounded-lg bg-pink-500 px-4 py-2 text-sm font-medium text-white hover:bg-pink-600 disabled:cursor-not-allowed disabled:bg-gray-300"
              onClick={() => void handleSubmit()}
            >
              {submitting ? "提交中..." : "开始生成"}
            </button>
          </div>
        </div>
      </div>
    );
  }

  // Base image confirmation phase
  if (phase === "base-confirm") {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
        <div className="w-[420px] rounded-2xl bg-white p-6 shadow-2xl">
          <h2 className="mb-4 text-center text-lg font-bold">确认基础形象</h2>
          <p className="mb-4 text-center text-sm text-gray-500">
            这是生成的宠物基础形象，确认后将基于它生成所有动画帧
          </p>
          {baseImageB64 && (
            <div className="mb-4 flex justify-center">
              <img
                src={`data:image/png;base64,${baseImageB64}`}
                alt="base pet"
                className="h-52 w-48 rounded-lg border border-gray-200 bg-gray-100 object-contain"
              />
            </div>
          )}
          <div className="flex justify-center gap-3">
            <button
              type="button"
              className="rounded-lg border border-gray-300 px-4 py-2 text-sm hover:bg-gray-50"
              onClick={() => void handleConfirmBase(false)}
            >
              不满意，重来
            </button>
            <button
              type="button"
              className="rounded-lg bg-pink-500 px-4 py-2 text-sm font-medium text-white hover:bg-pink-600"
              onClick={() => void handleConfirmBase(true)}
            >
              确认，继续生成
            </button>
          </div>
        </div>
      </div>
    );
  }

  // Generating / completed / failed phase
  return (
    <>
    {showLogs && <LogWindow logs={logs} logEndRef={logEndRef} onClose={() => setShowLogs(false)} />}
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-[420px] rounded-2xl bg-white p-6 shadow-2xl">
        <h2 className="mb-4 text-center text-lg font-bold">
          {phase === "completed"
            ? "宠物生成完成！"
            : phase === "failed"
              ? "生成失败"
              : "正在捏宠物..."}
        </h2>

        <div className="mb-4 max-h-[360px] overflow-y-auto">
          {steps.map((s) => (
            <div key={s.step} className="mb-1">
              <div className="flex items-center gap-2 py-1">
                <StepIcon status={s.status} />
                <span
                  className={`text-sm ${
                    s.status === "running"
                      ? "font-medium text-pink-600"
                      : s.status === "completed"
                        ? "text-gray-500"
                        : s.status === "failed"
                          ? "text-red-500"
                          : "text-gray-400"
                  }`}
                >
                  {s.label}
                </span>
                {s.status === "running" && <ElapsedTimer />}
              </div>
              {s.subSteps && s.status !== "pending" && (
                <div className="ml-6 border-l border-gray-100 pl-3">
                  {s.subSteps.map((sub) => (
                    <div
                      key={sub.name}
                      className="flex items-center gap-1.5 py-0.5"
                    >
                      <SubStepIcon status={sub.status} />
                      <span
                        className={`text-xs ${
                          sub.status === "running"
                            ? "text-pink-500"
                            : sub.status === "completed"
                              ? "text-gray-400"
                              : "text-gray-300"
                        }`}
                      >
                        {sub.name}
                      </span>
                      {sub.status === "running" && <ElapsedTimer />}
                    </div>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>

        <div className="mb-3 text-center">
          <button
            type="button"
            className="inline-flex items-center gap-1.5 rounded border border-gray-200 px-3 py-1 text-xs text-gray-500 hover:border-gray-300 hover:text-gray-700"
            onClick={() => setShowLogs(true)}
          >
            <span className="font-mono">⬡</span> 打开日志窗口 ({logs.length})
          </button>
        </div>

        {phase === "failed" && errorMessage && (
          <p className="mb-3 rounded-lg bg-red-50 px-3 py-2 text-xs text-red-600">
            {errorMessage}
          </p>
        )}

        <div className="flex justify-center gap-3">
          {phase === "completed" ? (
            <button
              type="button"
              className="rounded-lg bg-pink-500 px-5 py-2 text-sm font-medium text-white hover:bg-pink-600"
              onClick={() => {
                onCreated();
                onClose();
              }}
            >
              完成
            </button>
          ) : phase === "failed" ? (
            <>
              <button
                type="button"
                className="rounded-lg border border-gray-300 px-4 py-2 text-sm hover:bg-gray-50"
                onClick={() => {
                  handleReset();
                }}
              >
                返回修改
              </button>
              <button
                type="button"
                className="rounded-lg border border-gray-300 px-4 py-2 text-sm hover:bg-gray-50"
                onClick={onClose}
              >
                关闭
              </button>
            </>
          ) : (
            <button
              type="button"
              className="rounded-lg border border-red-300 px-4 py-2 text-sm text-red-600 hover:bg-red-50"
              onClick={() => void handleCancel()}
            >
              取消生成
            </button>
          )}
        </div>
      </div>
    </div>
    </>
  );
}

function LogWindow({
  logs,
  logEndRef,
  onClose,
}: {
  logs: LogEntry[];
  logEndRef: React.RefObject<HTMLDivElement>;
  onClose: () => void;
}) {
  const [pos, setPos] = useState({ x: 60, y: 60 });
  const dragging = useRef(false);
  const dragOffset = useRef({ x: 0, y: 0 });

  const handleMouseDown = (e: React.MouseEvent) => {
    dragging.current = true;
    dragOffset.current = { x: e.clientX - pos.x, y: e.clientY - pos.y };
    e.preventDefault();
  };

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!dragging.current) return;
      setPos({ x: e.clientX - dragOffset.current.x, y: e.clientY - dragOffset.current.y });
    };
    const handleMouseUp = () => {
      dragging.current = false;
    };
    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, []);

  return (
    <div
      className="fixed z-[100] flex flex-col rounded-xl border border-gray-700 bg-gray-900 shadow-2xl"
      style={{ left: pos.x, top: pos.y, width: 520, height: 320 }}
    >
      <div
        className="flex shrink-0 cursor-move items-center justify-between rounded-t-xl bg-gray-800 px-3 py-2"
        onMouseDown={handleMouseDown}
      >
        <span className="text-xs font-medium text-gray-300">生成日志</span>
        <button
          type="button"
          className="flex h-5 w-5 items-center justify-center rounded text-gray-400 hover:bg-gray-700 hover:text-white"
          onClick={onClose}
        >
          ×
        </button>
      </div>
      <div className="flex-1 overflow-y-auto p-3 font-mono text-[11px] leading-relaxed">
        {logs.length === 0 ? (
          <p className="text-gray-500">等待日志...</p>
        ) : (
          logs.map((log, idx) => (
            <div key={idx} className="whitespace-pre-wrap break-all">
              <span className="text-gray-500">{log.time}</span>{" "}
              <span
                className={
                  log.level === "error"
                    ? "text-red-400"
                    : log.level === "warn"
                      ? "text-yellow-400"
                      : log.level === "debug"
                        ? "text-gray-400"
                        : "text-green-300"
                }
              >
                [{log.level}]
              </span>{" "}
              <span className="text-gray-200">{log.message}</span>
            </div>
          ))
        )}
        <div ref={logEndRef} />
      </div>
    </div>
  );
}

function StepIcon({ status }: { status: string }) {
  if (status === "completed") {
    return (
      <span className="flex h-5 w-5 items-center justify-center rounded-full bg-green-100 text-xs text-green-600">
        ✓
      </span>
    );
  }
  if (status === "running") {
    return (
      <span className="flex h-5 w-5 items-center justify-center">
        <span className="h-4 w-4 animate-spin rounded-full border-2 border-pink-300 border-t-pink-600" />
      </span>
    );
  }
  if (status === "failed") {
    return (
      <span className="flex h-5 w-5 items-center justify-center rounded-full bg-red-100 text-xs text-red-600">
        ×
      </span>
    );
  }
  return (
    <span className="flex h-5 w-5 items-center justify-center rounded-full border border-gray-200 text-xs text-gray-300">
      ○
    </span>
  );
}

function SubStepIcon({ status }: { status: string }) {
  if (status === "completed") {
    return <span className="text-[10px] text-green-500">●</span>;
  }
  if (status === "running") {
    return (
      <span className="inline-block h-2 w-2 animate-pulse rounded-full bg-pink-400" />
    );
  }
  return <span className="text-[10px] text-gray-300">○</span>;
}

function ResumeTasksCollapsible({
  tasks,
  submitting,
  onResume,
  onRefresh,
}: {
  tasks: IncompleteCreationTask[];
  submitting: boolean;
  onResume: (workDirName: string) => void;
  onRefresh: () => void;
}) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="mb-4">
      <button
        type="button"
        className="flex w-full items-center gap-1.5 rounded-lg border border-amber-200 bg-amber-50 px-3 py-1.5 text-xs text-amber-700 transition hover:bg-amber-100"
        onClick={() => setExpanded((v) => !v)}
      >
        <span className="text-sm leading-none">{expanded ? "▾" : "▸"}</span>
        <span>有 {tasks.length} 个未完成任务可续跑</span>
      </button>
      {expanded && (
        <div className="mt-2 space-y-1.5 rounded-lg border border-gray-200 bg-gray-50 p-2">
          {tasks.map((task) => (
            <div
              key={task.workDirName}
              className="flex items-center justify-between rounded-md bg-white px-2 py-1.5"
            >
              <div className="min-w-0">
                <p className="truncate text-[11px] font-medium text-gray-700">{task.petName}</p>
                <p className="text-[10px] text-gray-400">
                  {task.completedCount}/{task.totalRows} 行
                </p>
              </div>
              <button
                type="button"
                disabled={submitting}
                className="shrink-0 rounded border border-pink-200 px-2 py-0.5 text-[11px] text-pink-600 hover:bg-pink-50 disabled:opacity-50"
                onClick={() => void onResume(task.workDirName)}
              >
                继续
              </button>
            </div>
          ))}
          <button
            type="button"
            className="w-full pt-1 text-center text-[10px] text-gray-400 hover:text-gray-600"
            onClick={() => void onRefresh()}
          >
            刷新列表
          </button>
        </div>
      )}
    </div>
  );
}

function ElapsedTimer() {
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    setElapsed(0);
    const id = setInterval(() => setElapsed((s) => s + 1), 1000);
    return () => clearInterval(id);
  }, []);
  const m = Math.floor(elapsed / 60);
  const s = elapsed % 60;
  const display = m > 0 ? `${m}m${s}s` : `${s}s`;
  return <span className="ml-1 font-mono text-[10px] text-gray-400">{display}</span>;
}
