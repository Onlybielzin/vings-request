// F10 — Campo de texto com realce de `{{var}}` (componente FINO).
//
// Envolve um <input> normal e renderiza, ABAIXO, um preview realcado dos tokens
// `{{var}}` do valor atual: verde = resolvido, vermelho = faltando, amarelo =
// secret (resolvido mas valor mascarado). A logica de tokenizacao/dica e PURA
// (src/lib/useVarHint.ts); aqui so renderizamos.
//
// Uso: <VarField value={url} onChange={setUrl} scopes={scopes} aria-label="URL" />
// Integracao pluga nos campos de URL e params/headers conforme desejar.
//
// SEGURANCA: o preview nunca mostra o valor de uma var `secret` (so o nome e o
// status). O valor real so existe no <input> que o usuario digitou (o nome da
// var, nao o segredo) e no envio interpolado.

import { useMemo } from "react";
import type { CSSProperties, InputHTMLAttributes } from "react";
import type { VarScopes } from "../lib/envScopes";
import { segmentar, dicaDoToken } from "../lib/useVarHint";

interface VarFieldProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, "onChange" | "value"> {
  value: string;
  onChange: (value: string) => void;
  scopes: VarScopes;
}

const estilos: Record<string, CSSProperties> = {
  wrap: { display: "flex", flexDirection: "column", gap: 2, width: "100%" },
  input: {
    width: "100%",
    padding: "4px 8px",
    fontSize: 12,
    fontFamily: "var(--mono, monospace)",
    color: "var(--fg, #e0e0e0)",
    background: "var(--bg, #1e1e1e)",
    border: "1px solid var(--border, #3a3a3a)",
    borderRadius: 4,
    boxSizing: "border-box",
  },
  preview: {
    fontSize: 11,
    fontFamily: "var(--mono, monospace)",
    whiteSpace: "pre-wrap",
    wordBreak: "break-all",
    lineHeight: 1.4,
    minHeight: 0,
  },
  tokenOk: { color: "#5fb568", fontWeight: 600 },
  tokenMissing: {
    color: "#e06c6c",
    fontWeight: 600,
    textDecoration: "underline wavy",
  },
  tokenSecret: { color: "#d6b34a", fontWeight: 600 },
};

/** Estilo do chip de um token conforme seu estado. */
function estiloToken(seg: {
  resolvido: boolean;
  secret: boolean;
}): CSSProperties {
  if (!seg.resolvido) return estilos.tokenMissing;
  if (seg.secret) return estilos.tokenSecret;
  return estilos.tokenOk;
}

export function VarField({
  value,
  onChange,
  scopes,
  ...rest
}: VarFieldProps) {
  const segmentos = useMemo(() => segmentar(value, scopes), [value, scopes]);
  const temTokens = segmentos.some((s) => s.tipo === "var");

  return (
    <div style={estilos.wrap}>
      <input
        {...rest}
        type={rest.type ?? "text"}
        style={{ ...estilos.input, ...(rest.style ?? {}) }}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
      {temTokens && (
        <div style={estilos.preview} aria-hidden="true">
          {segmentos.map((seg, i) =>
            seg.tipo === "texto" ? (
              <span key={i}>{seg.conteudo}</span>
            ) : (
              <span key={i} style={estiloToken(seg)} title={dicaDoToken(seg)}>
                {seg.bruto}
              </span>
            ),
          )}
        </div>
      )}
    </div>
  );
}

export default VarField;
