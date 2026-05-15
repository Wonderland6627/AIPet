import { NavLink, Route, Routes, Navigate } from "react-router-dom";

function PetLibrary() {
  return (
    <div>
      <h2 className="text-xl font-bold mb-4">宠物库</h2>
      <div className="grid grid-cols-3 gap-4">
        <div className="border rounded-lg p-4 cursor-pointer hover:shadow-md transition-shadow">
          <div className="w-24 h-26 bg-pink-100 rounded-lg mx-auto mb-2 flex items-center justify-center">
            <span className="text-sm text-pink-400 font-medium">D.Va</span>
          </div>
          <p className="text-center font-medium text-sm">D.Va</p>
          <p className="text-center text-xs text-gray-500">机甲驾驶员</p>
        </div>
      </div>
    </div>
  );
}

function RoleSettings() {
  return (
    <div>
      <h2 className="text-xl font-bold mb-4">职责设置</h2>
      <p className="text-gray-500 text-sm">
        角色和职责配置将在后续版本中开放。
      </p>
      <div className="mt-4 p-4 bg-gray-50 rounded-lg">
        <p className="text-sm text-gray-600">当前角色: 策划</p>
        <p className="text-xs text-gray-400 mt-1">
          进程映射: Excel.exe → 等待 | Cursor.exe → 工作中
        </p>
      </div>
    </div>
  );
}

function AppSettings() {
  return (
    <div>
      <h2 className="text-xl font-bold mb-4">应用设置</h2>
      <div className="space-y-4">
        <label className="flex items-center justify-between py-2">
          <span className="text-sm">窗口置顶</span>
          <input
            type="checkbox"
            defaultChecked
            className="w-4 h-4 accent-pink-500"
          />
        </label>
        <label className="flex items-center justify-between py-2">
          <span className="text-sm">开机自启</span>
          <input type="checkbox" className="w-4 h-4 accent-pink-500" />
        </label>
        <div className="flex items-center justify-between py-2">
          <span className="text-sm">动画速度</span>
          <select className="text-sm border rounded px-2 py-1">
            <option value="slow">慢</option>
            <option value="normal" selected>
              正常
            </option>
            <option value="fast">快</option>
          </select>
        </div>
      </div>
    </div>
  );
}

const NAV_ITEMS = [
  { to: "/pets", label: "宠物库" },
  { to: "/roles", label: "职责设置" },
  { to: "/app", label: "应用设置" },
];

export default function Settings() {
  return (
    <div className="flex h-screen bg-white text-gray-900">
      <nav className="w-44 bg-gray-50 border-r border-gray-200 p-4 flex flex-col gap-1">
        <h1 className="text-lg font-bold mb-4 px-3">AIPet</h1>
        {NAV_ITEMS.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            className={({ isActive }) =>
              `block px-3 py-2 rounded-lg text-sm transition-colors ${
                isActive
                  ? "bg-pink-100 text-pink-700 font-medium"
                  : "text-gray-600 hover:bg-gray-100"
              }`
            }
          >
            {item.label}
          </NavLink>
        ))}
      </nav>
      <main className="flex-1 p-6 overflow-y-auto">
        <Routes>
          <Route path="/pets" element={<PetLibrary />} />
          <Route path="/roles" element={<RoleSettings />} />
          <Route path="/app" element={<AppSettings />} />
          <Route path="*" element={<Navigate to="/pets" replace />} />
        </Routes>
      </main>
    </div>
  );
}
