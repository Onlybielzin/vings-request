// Input de texto com realce INLINE de `{{var}}` (F10 plugado na barra de URL).
//
// Tecnica: um <input> com texto transparente (so o cursor aparece) sobre uma
// camada colorida que renderiza os mesmos caracteres, com os tokens `{{var}}`
// destacados por cor segundo o estado (resolvida / nao resolvida / secret). O
// scroll horizontal das duas camadas e sincronizado. A tokenizacao e PURA
// (src/lib/useVarHint.ts).
//
// SEGURANCA: o realce nunca mostra o VALOR de uma var secret — so colore o token
// `{{nome}}` que o usuario ja digitou. O valor resolvido nunca aparece aqui.

import { useMemo, useRef } from "react";
import type { CSSProperties } from "react";
import { segmentar, dicaDoToken, type Segmento } from "../lib/useVarHint";
import type { VarScopes } from "../lib/envScopes";

interface Props {
  value: string;
  onChange: (value: string) => void;
  scopes: VarScopes;
  placeholder?: string;
  disabled?: boolean;
  ariaLabel?: string;
  style?: CSSProperties;
}

// Cores dos tokens (tema escuro). Variavel = sempre cor distinta do texto comum.
const COR_VAR_OK = "#c586c0"; // roxo: var resolvida
const COR_VAR_MISSING = "#e06c6c"; // vermelho: var nao resolvida
const COR_VAR_SECRET = "#d6b34a"; // ambar: var secret (resolvida, valor oculto)

function corDoToken(seg: Extract<Segmento, { tipo: "var" }>): string {
  if (!seg.resolvido) return COR_VAR_MISSING;
  if (seg.secret) return COR_VAR_SECRET;
  return COR_VAR_OK;
}

export function HighlightedInput({
  value,
  onChange,
  scopes,
  placeholder,
  disabled,
  ariaLabel,
  style,
}: Props) {
  const inputRef = useRef<HTMLInputElement>(null);
  const camadaRef = useRef<HTMLDivElement>(null);

  const segs = useMemo(() => segmentar(value, scopes), [value, scopes]);

  // Mantem a camada colorida alinhada com o scroll horizontal do input.
  const sincronizar = () => {
    if (inputRef.current && camadaRef.current) {
      camadaRef.current.scrollLeft = inputRef.current.scrollLeft;
    }
  };

  return (
    <div style={{ ...wrap, ...style }}>
      <div ref={camadaRef} aria-hidden style={camada}>
        {value === "" ? (
          <span style={{ color: "#6b6b6b" }}>{placeholder ?? ""}</span>
        ) : (
          segs.map((s, i) =>
            s.tipo === "texto" ? (
              <span key={i}>{s.conteudo}</span>
            ) : (
              <span key={i} style={{ color: corDoToken(s) }} title={dicaDoToken(s)}>
                {s.bruto}
              </span>
            ),
          )
        )}
      </div>
      <input
        ref={inputRef}
        type="text"
        aria-label={ariaLabel}
        placeholder={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onScroll={sincronizar}
        disabled={disabled}
        spellCheck={false}
        autoComplete="off"
        style={inputStyle}
      />
    </div>
  );
}

const wrap: CSSProperties = {
  position: "relative",
  flex: 1,
  minWidth: 0,
  background: "#1e1e1e",
  border: "1px solid #3a3a3a",
  borderRadius: 4,
  overflow: "hidden",
};

// Camada e input DEVEM ter a mesma fonte/padding para alinhar caractere a caractere.
const FONTE = "monospace";
const FONT_SIZE = 13;
const PADDING = "0.45rem 0.6rem";

const camada: CSSProperties = {
  position: "absolute",
  inset: 0,
  padding: PADDING,
  fontFamily: FONTE,
  fontSize: FONT_SIZE,
  lineHeight: "1.4",
  color: "#e0e0e0",
  whiteSpace: "pre",
  overflowX: "hidden",
  overflowY: "hidden",
  pointerEvents: "none",
  userSelect: "none",
};

const inputStyle: CSSProperties = {
  position: "relative",
  width: "100%",
  background: "transparent",
  border: "none",
  outline: "none",
  padding: PADDING,
  fontFamily: FONTE,
  fontSize: FONT_SIZE,
  lineHeight: "1.4",
  color: "transparent", // texto invisivel: a camada colorida aparece no lugar
  caretColor: "#e0e0e0", // cursor continua visivel
};

export default HighlightedInput;
