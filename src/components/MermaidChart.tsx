import { useEffect, useId, useState } from "react";

export default function MermaidChart({ chart, live = false }: { chart: string; live?: boolean }) {
  const reactId = useId().replace(/:/g, "");
  const [svg, setSvg] = useState("");
  const source = chart.trim();

  useEffect(() => {
    if (live || !source) return;
    let cancelled = false;
    void import("mermaid").then(async (mod) => {
      mod.default.initialize({
        startOnLoad: false,
        theme: "dark",
        securityLevel: "strict",
        suppressErrorRendering: true,
      });
      const parsed = await mod.default.parse(source, { suppressErrors: true });
      if (cancelled || parsed === false) return;
      const { svg: rendered } = await mod.default.render(`shiro-${reactId}-${Date.now()}`, source);
      if (!cancelled) setSvg(rendered);
    }).catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [source, live, reactId]);

  if (live || !svg) {
    return <pre className="whitespace-pre-wrap text-xs text-white/70">{chart}</pre>;
  }
  return (
    <div
      className="overflow-x-auto [&_svg]:max-w-full [&_svg]:h-auto"
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}
