// F9 — Dropdown do ambiente ativo (no topo do app). Componente FINO: le os
// environments da colecao ativa do envStore e seta o env ativo. Carrega os
// environments (do disco) e as globais ao trocar de colecao ativa.
//
// NAO montado no App.tsx aqui; a fase de Integracao posiciona no header.

import { useEffect } from "react";
import { useEnvStore } from "../store/envStore";
import { useCollectionsStore } from "../store/collectionsStore";

const estilos: Record<string, React.CSSProperties> = {
  wrap: {
    display: "flex",
    alignItems: "center",
    gap: 6,
  },
  label: {
    fontSize: 11,
    color: "var(--muted, #9a9a9a)",
  },
  select: {
    padding: "4px 8px",
    fontSize: 12,
    color: "var(--fg, #e0e0e0)",
    background: "var(--bg, #1e1e1e)",
    border: "1px solid var(--border, #3a3a3a)",
    borderRadius: 4,
    cursor: "pointer",
    minWidth: 140,
  },
};

/** Valor especial do <option> que representa "Nenhum ambiente". */
const SEM_AMBIENTE = "";

export function EnvSelector() {
  const activePath = useCollectionsStore((s) => s.activePath);
  const carregar = useEnvStore((s) => s.carregar);
  const setActiveEnv = useEnvStore((s) => s.setActiveEnv);
  const colState = useEnvStore((s) =>
    activePath ? s.porColecao[activePath] : undefined,
  );

  // Ao trocar de colecao ativa, garante que os environments dela estao carregados.
  useEffect(() => {
    if (activePath) void carregar(activePath);
  }, [activePath, carregar]);

  if (!activePath) return null;

  const environments = colState?.environments ?? [];
  const activeEnvName = colState?.activeEnvName ?? null;

  return (
    <div style={estilos.wrap}>
      <span style={estilos.label}>Ambiente</span>
      <select
        aria-label="Ambiente ativo"
        style={estilos.select}
        value={activeEnvName ?? SEM_AMBIENTE}
        onChange={(e) => {
          const v = e.target.value;
          setActiveEnv(activePath, v === SEM_AMBIENTE ? null : v);
        }}
      >
        <option value={SEM_AMBIENTE}>Nenhum</option>
        {environments.map((env) => (
          <option key={env.name} value={env.name}>
            {env.name}
          </option>
        ))}
      </select>
    </div>
  );
}

export default EnvSelector;
