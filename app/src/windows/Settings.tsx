import { NavLink, Route, Routes, Navigate } from "react-router-dom";
import { PetLibrarySection } from "./PetLibrarySection";
import { StateConfigSection } from "./StateConfigSection";
import { AppSettingsSection } from "./AppSettingsSection";
import { PetStatusPanel } from "./PetStatusPanel";

const NAV_ITEMS = [
  { to: "/pets", label: "宠物库" },
  { to: "/states", label: "状态配置" },
  { to: "/app", label: "应用设置" },
];

export default function Settings() {
  return (
    <div className="flex h-screen bg-white text-gray-900">
      <nav className="flex h-full w-44 shrink-0 flex-col gap-1 border-r border-gray-200 bg-gray-50 p-4">
        <h1 className="mb-4 px-3 text-lg font-bold">AIPet</h1>
        {NAV_ITEMS.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            className={({ isActive }) =>
              `block rounded-lg px-3 py-2 text-sm transition-colors ${
                isActive
                  ? "bg-pink-100 font-medium text-pink-700"
                  : "text-gray-600 hover:bg-gray-100"
              }`
            }
          >
            {item.label}
          </NavLink>
        ))}
        <PetStatusPanel />
      </nav>
      <main className="flex-1 overflow-y-auto p-6">
        <Routes>
          <Route path="/pets" element={<PetLibrarySection />} />
          <Route path="/states" element={<StateConfigSection />} />
          <Route path="/app" element={<AppSettingsSection />} />
          <Route path="*" element={<Navigate to="/pets" replace />} />
        </Routes>
      </main>
    </div>
  );
}
