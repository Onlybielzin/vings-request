// RequestBuilder (F4): dropdown de metodo + input de URL + botao Send.
// Componente FINO: delega toda a logica ao requestStore. Estilos inline pra nao
// depender de App.css (fora da propriedade desta feature); a fase de Integracao
// pode mover pra classes .app-* se quiser.

import { useMemo } from "react";
import type { ChangeEvent, FormEvent } from "react";
import { HTTP_METHODS } from "../lib/http-types";
import { useRequestStore } from "../store/requestStore";
import { useEnvStore } from "../store/envStore";
import { useCollectionsStore } from "../store/collectionsStore";
import { HighlightedInput } from "./HighlightedInput";

export function RequestBuilder() {
  const method = useRequestStore((s) => s.request.method);
  const url = useRequestStore((s) => s.request.url);
  const loading = useRequestStore((s) => s.loading);
  const error = useRequestStore((s) => s.error);
  const atualizarRequest = useRequestStore((s) => s.atualizarRequest);
  const enviar = useRequestStore((s) => s.enviar);

  // Escopos de variaveis da colecao ativa, para realcar `{{var}}` na URL.
  // IMPORTANTE: selecionamos uma referencia ESTAVEL (porColecao) e derivamos os
  // scopes via useMemo. Chamar `scopesDe(...)` direto no seletor retornaria um
  // objeto novo a cada render, violando o cache do useSyncExternalStore (zustand)
  // e causando loop infinito de render.
  const activePath = useCollectionsStore((s) => s.activePath);
  const porColecao = useEnvStore((s) => s.porColecao);
  const scopes = useMemo(
    () => useEnvStore.getState().scopesDe(activePath),
    [porColecao, activePath],
  );

  const onMethod = (e: ChangeEvent<HTMLSelectElement>) => {
    atualizarRequest({ method: e.target.value });
  };

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    void enviar();
  };

  const urlVazia = url.trim().length === 0;

  return (
    <form className="request-builder" onSubmit={onSubmit} style={estilos.form}>
      <div style={estilos.linha}>
        <select
          aria-label="Metodo HTTP"
          value={method}
          onChange={onMethod}
          disabled={loading}
          style={estilos.select}
        >
          {HTTP_METHODS.map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>

        <HighlightedInput
          ariaLabel="URL"
          placeholder="https://api.exemplo.com/recurso"
          value={url}
          onChange={(v) => atualizarRequest({ url: v })}
          scopes={scopes}
          disabled={loading}
        />

        <button
          type="submit"
          disabled={loading || urlVazia}
          style={estilos.botao}
        >
          {loading ? "Enviando..." : "Send"}
        </button>
      </div>

      {error !== null && (
        <div role="alert" className="request-builder-erro" style={estilos.erro}>
          {error}
        </div>
      )}
    </form>
  );
}

// Estilos inline minimos, tema escuro coerente com App.css.
const estilos: Record<string, React.CSSProperties> = {
  form: {
    display: "flex",
    flexDirection: "column",
    gap: "0.5rem",
    width: "100%",
  },
  linha: {
    display: "flex",
    gap: "0.5rem",
    alignItems: "stretch",
  },
  select: {
    background: "#1e1e1e",
    color: "#e0e0e0",
    border: "1px solid #3a3a3a",
    borderRadius: "4px",
    padding: "0 0.6rem",
    fontWeight: 600,
    cursor: "pointer",
  },
  input: {
    flex: 1,
    background: "#1e1e1e",
    color: "#e0e0e0",
    border: "1px solid #3a3a3a",
    borderRadius: "4px",
    padding: "0.45rem 0.6rem",
    fontFamily: "monospace",
  },
  botao: {
    background: "#3b82f6",
    color: "#fff",
    border: "none",
    borderRadius: "4px",
    padding: "0 1.1rem",
    fontWeight: 600,
    cursor: "pointer",
  },
  erro: {
    color: "#f87171",
    fontSize: "0.85rem",
    fontFamily: "monospace",
  },
};

export default RequestBuilder;
