import type { UpdateState } from "../hooks/useAppUpdater";

interface UpdateModalProps {
  state: UpdateState;
  onDownload: () => void;
  onRestart: () => void;
  onClose: () => void;
}

export function UpdateModal({ state, onDownload, onRestart, onClose }: UpdateModalProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-[420px] rounded-2xl bg-white p-6 shadow-2xl">
        {state.status === "available" && (
          <>
            <h3 className="mb-2 text-center text-lg font-bold">发现新版本 v{state.version}</h3>
            {state.body && (
              <p className="mb-4 max-h-40 overflow-y-auto whitespace-pre-wrap rounded-lg bg-gray-50 p-3 text-xs text-gray-600">
                {state.body}
              </p>
            )}
            <div className="flex justify-end gap-3">
              <button
                type="button"
                className="rounded-lg border border-gray-300 px-4 py-2 text-sm hover:bg-gray-50"
                onClick={onClose}
              >
                稍后再说
              </button>
              <button
                type="button"
                className="rounded-lg bg-pink-500 px-4 py-2 text-sm font-medium text-white hover:bg-pink-600"
                onClick={onDownload}
              >
                下载更新
              </button>
            </div>
          </>
        )}

        {state.status === "downloading" && (
          <>
            <h3 className="mb-4 text-center text-lg font-bold">正在下载更新…</h3>
            <div className="mb-2 h-2 w-full overflow-hidden rounded-full bg-gray-200">
              <div
                className="h-full rounded-full bg-pink-500 transition-all"
                style={{ width: `${state.progress}%` }}
              />
            </div>
            <p className="text-center text-xs text-gray-400">{state.progress}%</p>
          </>
        )}

        {state.status === "ready" && (
          <>
            <h3 className="mb-2 text-center text-lg font-bold">下载完成</h3>
            <p className="mb-4 text-center text-sm text-gray-500">重启应用后即可使用新版本 v{state.version}</p>
            <div className="flex justify-end gap-3">
              <button
                type="button"
                className="rounded-lg border border-gray-300 px-4 py-2 text-sm hover:bg-gray-50"
                onClick={onClose}
              >
                稍后重启
              </button>
              <button
                type="button"
                className="rounded-lg bg-pink-500 px-4 py-2 text-sm font-medium text-white hover:bg-pink-600"
                onClick={onRestart}
              >
                立即重启
              </button>
            </div>
          </>
        )}

        {state.status === "error" && (
          <>
            <h3 className="mb-2 text-center text-lg font-bold text-red-500">更新失败</h3>
            <p className="mb-4 break-all text-center text-xs text-gray-500">{state.error}</p>
            <div className="flex justify-end">
              <button
                type="button"
                className="rounded-lg border border-gray-300 px-4 py-2 text-sm hover:bg-gray-50"
                onClick={onClose}
              >
                关闭
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
