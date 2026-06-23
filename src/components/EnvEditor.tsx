// F9 — Editor de environments e variaveis (3 escopos: env ativo, colecao,
// global). CRUD de environments e suas variaveis; edicao das collection vars e
// das global vars. Componente FINO: delega persistencia ao envStore.
//
// Variaveis secret: o input do valor vira type=password (mascarado) e o valor
// nunca e exibido em claro nem logado. O toggle "secret" alterna o mascaramento.
//
// NAO montado no App.tsx aqui; a fase de Integracao posiciona (ex.: painel/modal).

import { useState } from "react";
import type { Environment, Variable } from "../lib/envScopes";
import {
  useEnvStore,
  novaVariavel,
  novoEnvironment,
} from "../store/envStore";
import { useCollectionsStore } from "../store/collectionsStore";

const estilos: Record<string, React.CSSProperties> = {
  wrap: {
    display: "flex",
    flexDirection: "column",
    gap: 12,
    padding: 12,
    color: "var(--fg, #e0e0e0)",
    fontSize: 13,
  },
  secao: {
    display: "flex",
    flexDirection: "column",
    gap: 6,
    border: "1px solid var(--border, #3a3a3a)",
    borderRadius: 6,
    padding: 10,
  },
  titulo: {
    fontWeight: 600,
    fontSize: 12,
    color: "var(--muted, #9a9a9a)",
    textTransform: "uppercase",
    letterSpacing: 0.5,
  },
  linhaTopo: {
    display: "flex",
    gap: 6,
    alignItems: "center",
    flexWrap: "wrap",
  },
  input: {
    padding: "5px 8px",
    fontSize: 12,
    color: "var(--fg, #e0e0e0)",
    background: "var(--bg, #1e1e1e)",
    border: "1px solid var(--border, #3a3a3a)",
    borderRadius: 4,
    fontFamily: "monospace",
  },
  inputFlex: {
    flex: 1,
    minWidth: 80,
  },
  select: {
    padding: "5px 8px",
    fontSize: 12,
    color: "var(--fg, #e0e0e0)",
    background: "var(--bg, #1e1e1e)",
    border: "1px solid var(--border, #3a3a3a)",
    borderRadius: 4,
  },
  botao: {
    padding: "5px 10px",
    fontSize: 12,
    color: "var(--fg, #e0e0e0)",
    background: "var(--bg, #1e1e1e)",
    border: "1px solid var(--border, #3a3a3a)",
    borderRadius: 4,
    cursor: "pointer",
  },
  remover: {
    background: "transparent",
    color: "#f87171",
    border: "1px solid var(--border, #3a3a3a)",
    borderRadius: 4,
    padding: "3px 8px",
    cursor: "pointer",
    lineHeight: 1,
  },
  linhaVar: {
    display: "flex",
    gap: 6,
    alignItems: "center",
  },
  vazio: {
    color: "#777",
    fontStyle: "italic",
    fontSize: 12,
  },
  erro: {
    color: "#f48771",
    fontSize: 12,
  },
};

/**
 * Editor de uma lista de variaveis (reusado por environment / colecao / global).
 * Recebe a lista e um callback que recebe a NOVA lista a cada mudanca.
 * `idBase` torna os aria-labels unicos por secao.
 */
function ListaVariaveis(props: {
  idBase: string;
  variables: Variable[];
  onChange: (vars: Variable[]) => void;
}) {
  const { idBase, variables, onChange } = props;

  const atualizar = (i: number, patch: Partial<Variable>) => {
    onChange(variables.map((v, idx) => (idx === i ? { ...v, ...patch } : v)));
  };
  const remover = (i: number) => {
    onChange(variables.filter((_, idx) => idx !== i));
  };
  const adicionar = () => {
    onChange([...variables, novaVariavel()]);
  };

  return (
    <>
      {variables.length === 0 && (
        <div style={estilos.vazio}>Nenhuma variavel.</div>
      )}
      {variables.map((v, i) => (
        <div key={i} style={estilos.linhaVar}>
          <input
            type="checkbox"
            aria-label={`Habilitar variavel ${idBase} ${i + 1}`}
            checked={v.enabled}
            onChange={(e) => atualizar(i, { enabled: e.target.checked })}
          />
          <input
            type="text"
            aria-label={`Nome da variavel ${idBase} ${i + 1}`}
            placeholder="nome"
            value={v.name}
            onChange={(e) => atualizar(i, { name: e.target.value })}
            style={{ ...estilos.input, ...estilos.inputFlex }}
            spellCheck={false}
            autoComplete="off"
          />
          <input
            // Variaveis secret: input mascarado, valor nunca em claro.
            type={v.secret ? "password" : "text"}
            aria-label={`Valor da variavel ${idBase} ${i + 1}`}
            placeholder="valor"
            value={v.value}
            onChange={(e) => atualizar(i, { value: e.target.value })}
            style={{ ...estilos.input, ...estilos.inputFlex }}
            spellCheck={false}
            autoComplete="off"
          />
          <label
            style={{ display: "flex", alignItems: "center", gap: 3 }}
            title="Marcar como secreta (mascara o valor)"
          >
            <input
              type="checkbox"
              aria-label={`Variavel secreta ${idBase} ${i + 1}`}
              checked={v.secret}
              onChange={(e) => atualizar(i, { secret: e.target.checked })}
            />
            <span style={{ fontSize: 11 }}>secret</span>
          </label>
          <button
            type="button"
            aria-label={`Remover variavel ${idBase} ${i + 1}`}
            title="Remover"
            onClick={() => remover(i)}
            style={estilos.remover}
          >
            x
          </button>
        </div>
      ))}
      <button type="button" style={estilos.botao} onClick={adicionar}>
        + Adicionar variavel
      </button>
    </>
  );
}

export function EnvEditor() {
  const activePath = useCollectionsStore((s) => s.activePath);
  const error = useEnvStore((s) => s.error);
  const colState = useEnvStore((s) =>
    activePath ? s.porColecao[activePath] : undefined,
  );
  const salvarEnvironment = useEnvStore((s) => s.salvarEnvironment);
  const excluirEnvironment = useEnvStore((s) => s.excluirEnvironment);
  const setCollectionVars = useEnvStore((s) => s.setCollectionVars);
  const setGlobalVars = useEnvStore((s) => s.setGlobalVars);

  // Nome do environment selecionado para edicao (separado do "ativo" do selector).
  const [envSelecionado, setEnvSelecionado] = useState<string | null>(null);
  const [novoNome, setNovoNome] = useState("");

  if (!activePath) {
    return (
      <div style={estilos.wrap}>
        <div style={estilos.vazio}>
          Abra uma colecao para editar ambientes e variaveis.
        </div>
      </div>
    );
  }

  const environments = colState?.environments ?? [];
  const collectionVars = colState?.collectionVars ?? [];
  const globalVars = colState?.globalVars ?? [];

  const envAtual: Environment | undefined =
    envSelecionado !== null
      ? environments.find((e) => e.name === envSelecionado)
      : undefined;

  const criarEnv = () => {
    const nome = novoNome.trim();
    if (!nome) return;
    void salvarEnvironment(activePath, novoEnvironment(nome));
    setEnvSelecionado(nome);
    setNovoNome("");
  };

  const salvarVarsDoEnv = (vars: Variable[]) => {
    if (!envAtual) return;
    void salvarEnvironment(activePath, { ...envAtual, variables: vars });
  };

  const excluirEnvAtual = () => {
    if (envSelecionado === null) return;
    void excluirEnvironment(activePath, envSelecionado);
    setEnvSelecionado(null);
  };

  return (
    <div style={estilos.wrap}>
      {error ? (
        <div style={estilos.erro} role="alert">
          {error}
        </div>
      ) : null}

      {/* ---- Environments ---- */}
      <section style={estilos.secao}>
        <div style={estilos.titulo}>Ambientes</div>
        <div style={estilos.linhaTopo}>
          <select
            aria-label="Selecionar ambiente para editar"
            style={estilos.select}
            value={envSelecionado ?? ""}
            onChange={(e) =>
              setEnvSelecionado(e.target.value === "" ? null : e.target.value)
            }
          >
            <option value="">Selecione um ambiente</option>
            {environments.map((env) => (
              <option key={env.name} value={env.name}>
                {env.name}
              </option>
            ))}
          </select>
          {envSelecionado !== null && (
            <button
              type="button"
              style={estilos.remover}
              onClick={excluirEnvAtual}
            >
              Excluir ambiente
            </button>
          )}
        </div>

        <div style={estilos.linhaTopo}>
          <input
            type="text"
            aria-label="Nome do novo ambiente"
            placeholder="Novo ambiente (ex.: Producao)"
            value={novoNome}
            onChange={(e) => setNovoNome(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") criarEnv();
            }}
            style={{ ...estilos.input, ...estilos.inputFlex }}
          />
          <button
            type="button"
            style={estilos.botao}
            onClick={criarEnv}
            disabled={novoNome.trim().length === 0}
          >
            Criar ambiente
          </button>
        </div>

        {envAtual ? (
          <ListaVariaveis
            idBase={`env-${envAtual.name}`}
            variables={envAtual.variables}
            onChange={salvarVarsDoEnv}
          />
        ) : (
          <div style={estilos.vazio}>
            Selecione ou crie um ambiente para editar suas variaveis.
          </div>
        )}
      </section>

      {/* ---- Collection vars ---- */}
      <section style={estilos.secao}>
        <div style={estilos.titulo}>Variaveis da colecao</div>
        <ListaVariaveis
          idBase="col"
          variables={collectionVars}
          onChange={(vars) => setCollectionVars(activePath, vars)}
        />
      </section>

      {/* ---- Global vars ---- */}
      <section style={estilos.secao}>
        <div style={estilos.titulo}>Variaveis globais (app)</div>
        <ListaVariaveis
          idBase="glob"
          variables={globalVars}
          onChange={(vars) => void setGlobalVars(activePath, vars)}
        />
      </section>
    </div>
  );
}

export default EnvEditor;
