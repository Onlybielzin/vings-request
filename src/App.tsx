import { useEffect, useMemo, useRef, useState } from "react";
import "./App.css";

import CollectionToolbar from "./components/CollectionToolbar";
import Sidebar from "./components/Sidebar";
import RequestBuilder from "./components/RequestBuilder";
import QueryParams from "./components/QueryParams";
import Headers from "./components/Headers";
import BodyEditor from "./components/BodyEditor";
import AuthTab from "./components/AuthTab";
import ResponseViewer from "./components/ResponseViewer";
import EnvSelector from "./components/EnvSelector";
import EnvEditor from "./components/EnvEditor";
import Tabs from "./components/Tabs";
import ScriptEditor from "./components/ScriptEditor";
import TestsPanel from "./components/TestsPanel";
import ScriptConsole from "./components/ScriptConsole";
import CookiesPanel from "./components/CookiesPanel";
import HistoryPanel from "./components/HistoryPanel";
import CodeGenPanel from "./components/CodeGenPanel";
import { RequestSettings } from "./components/RequestSettings";
import { SettingsPanel } from "./components/SettingsPanel";
import ImportExportPanel from "./components/ImportExportPanel";
import McpPanel from "./components/McpPanel";
import CommandPalette from "./components/CommandPalette";
import GlobalSearch from "./components/GlobalSearch";

import { useCollectionsStore } from "./store/collectionsStore";
import { useRequestStore } from "./store/requestStore";
import { useTabsStore } from "./store/tabsStore";
import { useCookiesStore, hostDeUrl } from "./store/cookiesStore";
import { useSettingsStore } from "./store/settingsStore";
import { useAtalhos, type HandlersAtalho } from "./lib/useAtalhos";
import { acharRequestPorItemPath } from "./lib/treeLookup";
import { saveRequest } from "./lib/ipc";
import type { Comando } from "./lib/search";
import type { RequestItem } from "./lib/types";

type AbaRequest =
  | "params"
  | "headers"
  | "body"
  | "auth"
  | "script"
  | "tests"
  | "settings"
  | "code";

const ABAS: { id: AbaRequest; rotulo: string }[] = [
  { id: "params", rotulo: "Params" },
  { id: "headers", rotulo: "Headers" },
  { id: "body", rotulo: "Body" },
  { id: "auth", rotulo: "Auth" },
  { id: "script", rotulo: "Script" },
  { id: "tests", rotulo: "Tests" },
  { id: "settings", rotulo: "Settings" },
  { id: "code", rotulo: "Code" },
];

type AbaPainel = "console" | "cookies" | "history";

const PAINEIS: { id: AbaPainel; rotulo: string }[] = [
  { id: "console", rotulo: "Console" },
  { id: "cookies", rotulo: "Cookies" },
  { id: "history", rotulo: "Historico" },
];

function App() {
  const restaurarColecoes = useCollectionsStore((s) => s.restaurarColecoes);
  const [aba, setAba] = useState<AbaRequest>("params");
  const [painel, setPainel] = useState<AbaPainel>("console");
  // Painel de variaveis/ambientes (EnvEditor) aberto sob demanda.
  const [varsAberto, setVarsAberto] = useState(false);
  // F17 — painel de import/export aberto sob demanda (lateral, como Variaveis).
  const [ioAberto, setIoAberto] = useState(false);
  // F20 — painel de configuracoes globais aberto sob demanda.
  const [settingsAberto, setSettingsAberto] = useState(false);
  // MCP — painel de autoconfiguracao do servidor MCP (conectar IA) sob demanda.
  const [mcpAberto, setMcpAberto] = useState(false);
  // F19 — command palette (Ctrl+K). Estado e dono dos atalhos ficam no App.
  const [paletteAberto, setPaletteAberto] = useState(false);

  // F20 — tema e fonte globais, aplicados no root (.app-shell).
  const theme = useSettingsStore((s) => s.settings.theme);
  const fontSize = useSettingsStore((s) => s.settings.fontSize);
  // F10 — nomes de variaveis nao resolvidas no ultimo envio (so NOMES; nunca
  // valores/secrets). Aviso NAO bloqueante.
  const avisoVars = useRequestStore((s) => s.avisoVars);

  // ---- F15: costura aba-ativa <-> requestStore ----------------------------
  const activeId = useTabsStore((s) => s.activeId);
  const request = useRequestStore((s) => s.request);

  // Ao trocar de aba ativa, carrega o snapshot da aba no builder.
  // Guardamos o ultimo id "espelhado" para distinguir troca-de-aba (carregar do
  // snapshot) de edicao-na-mesma-aba (espelhar de volta).
  const idEspelhado = useRef<string | null>(null);
  useEffect(() => {
    if (activeId === idEspelhado.current) return;
    idEspelhado.current = activeId;
    if (activeId === null) return;
    const aba = useTabsStore.getState().tabs.find((t) => t.id === activeId);
    if (aba) {
      useRequestStore.getState().setRequest(aba.request);
    }
  }, [activeId]);

  // Ao editar a request da aba ativa, espelha de volta no snapshot da aba
  // (marca suja). So espelhamos quando a edicao e na MESMA aba ja carregada —
  // evita marcar suja na troca de aba (que tambem muda `request`).
  useEffect(() => {
    const idAtual = useTabsStore.getState().activeId;
    if (idAtual === null || idAtual !== idEspelhado.current) return;
    useTabsStore.getState().atualizarRequestAtiva(request);
  }, [request]);

  // ---- F14: registra o host de cada envio concluido (para listar cookies) --
  const loading = useRequestStore((s) => s.loading);
  const loadingAnterior = useRef(loading);
  useEffect(() => {
    const terminou = loadingAnterior.current && !loading;
    loadingAnterior.current = loading;
    if (!terminou) return;
    const st = useRequestStore.getState();
    const host = hostDeUrl(st.request.url);
    if (host) useCookiesStore.getState().registrarDominio(host);
  }, [loading]);

  // ---- Boot: restaura colecoes e, em seguida, as abas da sessao -----------
  useEffect(() => {
    void (async () => {
      await restaurarColecoes();
      useTabsStore.getState().restaurar((collectionPath, itemPath) => {
        if (collectionPath === null) return null;
        const col = useCollectionsStore.getState().collections[collectionPath];
        return acharRequestPorItemPath(col, itemPath);
      });
      // Se uma aba foi restaurada como ativa, carrega-a no builder.
      const ativa = useTabsStore.getState().activeId;
      if (ativa !== null) {
        const aba = useTabsStore.getState().tabs.find((t) => t.id === ativa);
        if (aba) {
          idEspelhado.current = ativa;
          useRequestStore.getState().setRequest(aba.request);
        }
      }
    })();
  }, [restaurarColecoes]);

  // ---- F15: atalhos de teclado --------------------------------------------
  const handlers: HandlersAtalho = useMemo(
    () => ({
      novaAba: () => {
        const id = useTabsStore.getState().abrirNova();
        idEspelhado.current = id;
        const aba = useTabsStore.getState().tabs.find((t) => t.id === id);
        if (aba) useRequestStore.getState().setRequest(aba.request);
      },
      fecharAba: () => {
        const id = useTabsStore.getState().activeId;
        if (id !== null) useTabsStore.getState().fecharAba(id);
      },
      salvar: () => void salvarRequestAtiva(),
      enviar: () => void useRequestStore.getState().enviar(),
    }),
    [],
  );
  useAtalhos(handlers);

  // ---- F19: atalho global Ctrl/Cmd+K abre o command palette ----------------
  // useAtalhos (F15) nao trata K; registramos um listener proprio aqui (App e
  // dono dos atalhos globais). Toggla o overlay.
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && !e.altKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteAberto((v) => !v);
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);

  // ---- F19: acoes reais do command palette --------------------------------
  // Injetadas no CommandPalette. `run` fala diretamente com os stores; o palette
  // fecha sozinho apos executar (onFechar no componente).
  const comandos = useMemo<Comando[]>(
    () => [
      {
        id: "nova-aba",
        label: "Nova aba (request avulsa)",
        keywords: ["new", "tab", "aba", "request", "criar"],
        secao: "Abas",
        run: () => {
          const id = useTabsStore.getState().abrirNova();
          idEspelhado.current = id;
          const a = useTabsStore.getState().tabs.find((t) => t.id === id);
          if (a) useRequestStore.getState().setRequest(a.request);
        },
      },
      {
        id: "nova-colecao",
        label: "Nova colecao...",
        keywords: ["new", "collection", "colecao", "criar", "abrir"],
        secao: "Colecoes",
        run: () => {
          void useCollectionsStore.getState().abrirColecao();
        },
      },
      {
        id: "enviar",
        label: "Enviar request atual",
        keywords: ["send", "enviar", "run", "executar"],
        secao: "Request",
        run: () => {
          void useRequestStore.getState().enviar();
        },
      },
      {
        id: "salvar",
        label: "Salvar request atual",
        keywords: ["save", "salvar"],
        secao: "Request",
        run: () => {
          void salvarRequestAtiva();
        },
      },
      {
        id: "abrir-settings",
        label: "Abrir configuracoes globais",
        keywords: ["settings", "config", "configuracoes", "proxy", "ssl", "tema"],
        secao: "App",
        run: () => setSettingsAberto(true),
      },
      {
        id: "abrir-import",
        label: "Importar / Exportar colecao",
        keywords: ["import", "export", "importar", "exportar", "postman", "curl", "openapi"],
        secao: "App",
        run: () => setIoAberto(true),
      },
    ],
    [],
  );

  return (
    <main className="app-shell" data-theme={theme} style={{ fontSize }}>
      <header className="app-header">
        <span className="app-title">Vings Request</span>
        <div className="app-header-tools">
          <EnvSelector />
          <button
            type="button"
            className="app-vars-btn"
            aria-pressed={varsAberto}
            onClick={() => setVarsAberto((v) => !v)}
          >
            Variaveis
          </button>
          <button
            type="button"
            className="app-vars-btn"
            aria-pressed={ioAberto}
            onClick={() => setIoAberto((v) => !v)}
          >
            Importar/Exportar
          </button>
          <button
            type="button"
            className="app-vars-btn"
            aria-pressed={settingsAberto}
            onClick={() => setSettingsAberto((v) => !v)}
          >
            Settings
          </button>
          <button
            type="button"
            className="app-vars-btn"
            aria-pressed={mcpAberto}
            onClick={() => setMcpAberto((v) => !v)}
          >
            IA / MCP
          </button>
        </div>
      </header>
      <section className="app-body">
        <aside className="app-sidebar" aria-label="Colecoes">
          <CollectionToolbar />
          <div className="app-sidebar-search">
            <GlobalSearch />
          </div>
          <Sidebar />
        </aside>
        <div className="app-main">
          <Tabs />

          <div className="rq-builder-wrap">
            <RequestBuilder />
          </div>

          <nav className="rq-tabs" role="tablist" aria-label="Editor da request">
            {ABAS.map((a) => (
              <button
                key={a.id}
                type="button"
                role="tab"
                aria-selected={aba === a.id}
                className={`rq-tab ${aba === a.id ? "rq-tab-active" : ""}`}
                onClick={() => setAba(a.id)}
              >
                {a.rotulo}
              </button>
            ))}
          </nav>

          <div className="rq-tab-panel" role="tabpanel">
            {aba === "params" && <QueryParams />}
            {aba === "headers" && <Headers />}
            {aba === "body" && <BodyEditor />}
            {aba === "auth" && <AuthTab />}
            {aba === "script" && <ScriptEditor />}
            {aba === "tests" && <TestsPanel />}
            {aba === "settings" && <RequestSettings />}
            {aba === "code" && <CodeGenPanel />}
          </div>

          {avisoVars.length > 0 && (
            <div className="rq-aviso-vars" role="status">
              Variaveis nao resolvidas: {avisoVars.join(", ")}
            </div>
          )}

          <div className="rq-response">
            <ResponseViewer />
          </div>

          <nav
            className="rq-tabs"
            role="tablist"
            aria-label="Paineis auxiliares"
          >
            {PAINEIS.map((p) => (
              <button
                key={p.id}
                type="button"
                role="tab"
                aria-selected={painel === p.id}
                className={`rq-tab ${painel === p.id ? "rq-tab-active" : ""}`}
                onClick={() => setPainel(p.id)}
              >
                {p.rotulo}
              </button>
            ))}
          </nav>

          <div className="rq-tab-panel" role="tabpanel">
            {painel === "console" && <ScriptConsole />}
            {painel === "cookies" && <CookiesPanel />}
            {painel === "history" && <HistoryPanel />}
          </div>
        </div>

        {varsAberto && (
          <aside className="app-vars-panel" aria-label="Variaveis e ambientes">
            <div className="app-vars-panel-head">
              <span>Variaveis e ambientes</span>
              <button
                type="button"
                className="app-vars-close"
                aria-label="Fechar painel de variaveis"
                onClick={() => setVarsAberto(false)}
              >
                x
              </button>
            </div>
            <EnvEditor />
          </aside>
        )}

        {ioAberto && (
          <aside className="app-vars-panel" aria-label="Importar e exportar">
            <div className="app-vars-panel-head">
              <span>Importar / Exportar</span>
              <button
                type="button"
                className="app-vars-close"
                aria-label="Fechar painel de import/export"
                onClick={() => setIoAberto(false)}
              >
                x
              </button>
            </div>
            <ImportExportPanel
              onImported={(path) => {
                useCollectionsStore.getState().setActive(path);
              }}
            />
          </aside>
        )}

        {settingsAberto && (
          <aside className="app-vars-panel" aria-label="Configuracoes globais">
            <div className="app-vars-panel-head">
              <span>Configuracoes</span>
              <button
                type="button"
                className="app-vars-close"
                aria-label="Fechar painel de configuracoes"
                onClick={() => setSettingsAberto(false)}
              >
                x
              </button>
            </div>
            <SettingsPanel />
          </aside>
        )}

        {mcpAberto && (
          <aside className="app-vars-panel" aria-label="Conectar IA e MCP">
            <div className="app-vars-panel-head">
              <span>IA / MCP</span>
              <button
                type="button"
                className="app-vars-close"
                aria-label="Fechar painel de IA/MCP"
                onClick={() => setMcpAberto(false)}
              >
                x
              </button>
            </div>
            <McpPanel />
          </aside>
        )}
      </section>

      <CommandPalette
        aberto={paletteAberto}
        onFechar={() => setPaletteAberto(false)}
        comandos={comandos}
      />
    </main>
  );
}

/**
 * Persiste a request da aba ativa no disco (Ctrl+S). So salva abas ligadas a uma
 * request de colecao (collectionPath/itemPath conhecidos); abas avulsas (ainda
 * nao salvas na arvore) sao ignoradas — a criacao de arquivo novo e do fluxo da
 * Sidebar. Apos salvar, limpa o "dot" de nao-salvo da aba.
 */
async function salvarRequestAtiva(): Promise<void> {
  const tabsState = useTabsStore.getState();
  const id = tabsState.activeId;
  if (id === null) return;
  const aba = tabsState.tabs.find((t) => t.id === id);
  if (!aba || aba.collectionPath === null || aba.itemPath === null) return;

  // O `itemPath` e "slugs/da/pasta/slug-da-request"; o `dir` para o save e tudo
  // menos o ultimo segmento (a request).
  const segmentos = aba.itemPath.split("/").filter((s) => s.length > 0);
  const dir =
    segmentos.length > 1 ? segmentos.slice(0, -1).join("/") : undefined;

  // Salva o estado ATUAL do builder (a aba ativa espelha a request em edicao).
  const request: RequestItem = useRequestStore.getState().request;
  try {
    await saveRequest(aba.collectionPath, request, dir);
    useTabsStore.getState().atualizarRequestDaAba(id, request, false);
  } catch {
    // Erro de escrita: mantem a aba suja; nao derruba a UI.
  }
}

export default App;
