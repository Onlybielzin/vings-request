import { useEffect, useState } from "react";
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
import { useCollectionsStore } from "./store/collectionsStore";
import { useRequestStore } from "./store/requestStore";

type AbaRequest = "params" | "headers" | "body" | "auth";

const ABAS: { id: AbaRequest; rotulo: string }[] = [
  { id: "params", rotulo: "Params" },
  { id: "headers", rotulo: "Headers" },
  { id: "body", rotulo: "Body" },
  { id: "auth", rotulo: "Auth" },
];

function App() {
  const restaurarColecoes = useCollectionsStore((s) => s.restaurarColecoes);
  const [aba, setAba] = useState<AbaRequest>("params");
  // Painel de variaveis/ambientes (EnvEditor) aberto sob demanda.
  const [varsAberto, setVarsAberto] = useState(false);
  // F10 — nomes de variaveis nao resolvidas no ultimo envio (so NOMES; nunca
  // valores/secrets). Aviso NAO bloqueante.
  const avisoVars = useRequestStore((s) => s.avisoVars);

  // Reabre as colecoes persistidas da sessao anterior, uma vez no start.
  useEffect(() => {
    void restaurarColecoes();
  }, [restaurarColecoes]);

  return (
    <main className="app-shell">
      <header className="app-header">
        <span className="app-title">ruan</span>
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
        </div>
      </header>
      <section className="app-body">
        <aside className="app-sidebar" aria-label="Colecoes">
          <CollectionToolbar />
          <Sidebar />
        </aside>
        <div className="app-main">
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
          </div>

          {avisoVars.length > 0 && (
            <div className="rq-aviso-vars" role="status">
              Variaveis nao resolvidas: {avisoVars.join(", ")}
            </div>
          )}

          <div className="rq-response">
            <ResponseViewer />
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
      </section>
    </main>
  );
}

export default App;
