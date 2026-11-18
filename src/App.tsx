import SourceHub from "./pages/SourceHub";

export default function App() {
  return (
    <div className="absolute inset-0 bg-bg text-on flex flex-col">
      <header className="shrink-0 px-4 py-3 border-b border-white/10 flex items-center justify-between"
        style={{ paddingTop: "calc(env(safe-area-inset-top, 0px) + 12px)" }}>
        <div>
          <div className="text-lg font-bold tracking-tight">MyVideo</div>
          <div className="text-[11px] text-mut">多站视频 · 搜索 · 播放 · 离线</div>
        </div>
        <div className="text-[11px] text-mut">独立版本</div>
      </header>
      <main className="flex-1 min-h-0 scroll-y px-4 py-3 pb-10">
        <SourceHub />
      </main>
    </div>
  );
}
