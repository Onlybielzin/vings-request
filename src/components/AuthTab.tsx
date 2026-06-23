// AuthTab (F11): seletor de tipo de autenticacao + campos por tipo.
// Componente FINO: le/escreve request.auth via requestStore.atualizarRequest.
// A logica pura de producao de headers/query vive em src/lib/auth.ts; aqui so
// editamos o objeto Auth e, para oauth2, chamamos o comando Rust `oauth2_token`.
//
// SEGURANCA (UI):
//   - Campos de credencial (password, token, client_secret, value de apikey)
//     usam type=password para nao exibir em claro na tela.
//   - O accessToken obtido e guardado em request.auth.token (em memoria) e
//     exibido mascarado; nunca e logado.

import { useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useRequestStore } from "../store/requestStore";
import type { Auth, AuthMode, ApiKeyPlacement } from "../lib/types";

// Rotulos amigaveis por modo (UI). Ordem do seletor.
const MODOS: Array<{ valor: AuthMode; rotulo: string }> = [
  { valor: "none", rotulo: "Sem auth" },
  { valor: "inherit", rotulo: "Herdar (pasta/colecao)" },
  { valor: "basic", rotulo: "Basic" },
  { valor: "bearer", rotulo: "Bearer Token" },
  { valor: "apikey", rotulo: "API Key" },
  { valor: "oauth2", rotulo: "OAuth 2.0" },
];

/** Grants OAuth2 suportados pelo comando Rust. */
const GRANTS = [
  { valor: "client_credentials", rotulo: "Client Credentials" },
  { valor: "authorization_code", rotulo: "Authorization Code" },
  { valor: "password", rotulo: "Password (ROPC)" },
  { valor: "refresh_token", rotulo: "Refresh Token" },
];

export function AuthTab() {
  const auth = useRequestStore((s) => s.request.auth);
  const atualizarRequest = useRequestStore((s) => s.atualizarRequest);

  // Aplica um patch parcial no Auth, preservando o resto.
  const patchAuth = (patch: Partial<Auth>) => {
    atualizarRequest({ auth: { ...auth, ...patch } });
  };

  const onTrocarModo = (modo: AuthMode) => {
    // Ao trocar o modo, mantemos os campos ja preenchidos (o usuario pode
    // alternar sem perder o que digitou); so o `mode` muda.
    patchAuth({ mode: modo });
  };

  return (
    <div className="auth-tab" style={estilos.container}>
      <div style={estilos.barra}>
        <label style={estilos.label} htmlFor="auth-mode">
          Autenticacao
        </label>
        <select
          id="auth-mode"
          aria-label="Tipo de autenticacao"
          value={auth.mode}
          onChange={(e) => onTrocarModo(e.target.value as AuthMode)}
          style={estilos.select}
        >
          {MODOS.map((m) => (
            <option key={m.valor} value={m.valor}>
              {m.rotulo}
            </option>
          ))}
        </select>
      </div>

      {auth.mode === "none" && (
        <p style={estilos.vazio}>Esta request nao envia autenticacao.</p>
      )}

      {auth.mode === "inherit" && (
        <p style={estilos.vazio}>
          Herda a autenticacao definida na pasta ou na colecao.
        </p>
      )}

      {auth.mode === "basic" && (
        <CamposBasic auth={auth} patch={patchAuth} />
      )}

      {auth.mode === "bearer" && (
        <Campo
          rotulo="Token"
          valor={auth.token ?? ""}
          onChange={(v) => patchAuth({ token: v })}
          segredo
          placeholder="{{token}} ou valor"
        />
      )}

      {auth.mode === "apikey" && (
        <CamposApiKey auth={auth} patch={patchAuth} />
      )}

      {auth.mode === "oauth2" && (
        <CamposOAuth2 auth={auth} patch={patchAuth} />
      )}
    </div>
  );
}

// ---- Basic ----------------------------------------------------------------

function CamposBasic(props: {
  auth: Auth;
  patch: (p: Partial<Auth>) => void;
}) {
  const { auth, patch } = props;
  return (
    <div style={estilos.campos}>
      <Campo
        rotulo="Usuario"
        valor={auth.username ?? ""}
        onChange={(v) => patch({ username: v })}
        placeholder="usuario ou {{user}}"
      />
      <Campo
        rotulo="Senha"
        valor={auth.password ?? ""}
        onChange={(v) => patch({ password: v })}
        segredo
        placeholder="senha ou {{pass}}"
      />
    </div>
  );
}

// ---- API Key --------------------------------------------------------------

function CamposApiKey(props: {
  auth: Auth;
  patch: (p: Partial<Auth>) => void;
}) {
  const { auth, patch } = props;
  const placement: ApiKeyPlacement = auth.placement ?? "header";
  return (
    <div style={estilos.campos}>
      <Campo
        rotulo="Chave"
        valor={auth.key ?? ""}
        onChange={(v) => patch({ key: v })}
        placeholder="X-API-Key ou api_key"
      />
      <Campo
        rotulo="Valor"
        valor={auth.value ?? ""}
        onChange={(v) => patch({ value: v })}
        segredo
        placeholder="valor ou {{apiKey}}"
      />
      <div style={estilos.linha}>
        <label style={estilos.label} htmlFor="apikey-placement">
          Enviar em
        </label>
        <select
          id="apikey-placement"
          aria-label="Local da API key"
          value={placement}
          onChange={(e) =>
            patch({ placement: e.target.value as ApiKeyPlacement })
          }
          style={estilos.select}
        >
          <option value="header">Header</option>
          <option value="query">Query param</option>
        </select>
      </div>
    </div>
  );
}

// ---- OAuth2 ---------------------------------------------------------------

/** Config do comando Rust oauth2_token (camelCase). */
interface OAuth2Config {
  grantType: string;
  tokenUrl: string;
  clientId?: string;
  clientSecret?: string;
  scope?: string;
  code?: string;
  redirectUri?: string;
  username?: string;
  password?: string;
  refreshToken?: string;
  clientAuth?: "body" | "basic";
}

interface OAuth2Token {
  accessToken: string;
  tokenType?: string;
  expiresIn?: number;
  refreshToken?: string;
  scope?: string;
}

interface OAuth2Error {
  kind: string;
  message: string;
}

function ehOAuthError(e: unknown): e is OAuth2Error {
  return (
    typeof e === "object" &&
    e !== null &&
    "message" in e &&
    typeof (e as OAuth2Error).message === "string"
  );
}

function CamposOAuth2(props: {
  auth: Auth;
  patch: (p: Partial<Auth>) => void;
}) {
  const { auth, patch } = props;
  // Estado local do formulario OAuth2 (config nao persiste no Auth do M2; so o
  // accessToken obtido vira auth.token). Mantido na UI para o botao "Obter token".
  const [grant, setGrant] = useState("client_credentials");
  const [tokenUrl, setTokenUrl] = useState("");
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [scope, setScope] = useState("");
  const [code, setCode] = useState("");
  const [redirectUri, setRedirectUri] = useState("");
  const [usuario, setUsuario] = useState("");
  const [senha, setSenha] = useState("");
  const [refreshToken, setRefreshToken] = useState("");
  const [clientAuth, setClientAuth] = useState<"body" | "basic">("body");

  const [carregando, setCarregando] = useState(false);
  const [erro, setErro] = useState<string | null>(null);
  const [ok, setOk] = useState<string | null>(null);

  const obterToken = async () => {
    setCarregando(true);
    setErro(null);
    setOk(null);
    const config: OAuth2Config = {
      grantType: grant,
      tokenUrl,
      clientId: clientId || undefined,
      clientSecret: clientSecret || undefined,
      scope: scope || undefined,
      code: code || undefined,
      redirectUri: redirectUri || undefined,
      username: usuario || undefined,
      password: senha || undefined,
      refreshToken: refreshToken || undefined,
      clientAuth,
    };
    try {
      const token = await invoke<OAuth2Token>("oauth2_token", { config });
      // Guarda o accessToken no Auth: o envio usa via aplicarAuth (Bearer).
      patch({ token: token.accessToken });
      const exp = token.expiresIn ? ` (expira em ${token.expiresIn}s)` : "";
      setOk(`Token obtido${exp}.`);
    } catch (e) {
      setErro(ehOAuthError(e) ? e.message : String(e));
    } finally {
      setCarregando(false);
    }
  };

  return (
    <div style={estilos.campos}>
      <div style={estilos.linha}>
        <label style={estilos.label} htmlFor="oauth-grant">
          Grant
        </label>
        <select
          id="oauth-grant"
          aria-label="Grant OAuth2"
          value={grant}
          onChange={(e) => setGrant(e.target.value)}
          style={estilos.select}
        >
          {GRANTS.map((g) => (
            <option key={g.valor} value={g.valor}>
              {g.rotulo}
            </option>
          ))}
        </select>
      </div>

      <Campo
        rotulo="Token URL"
        valor={tokenUrl}
        onChange={setTokenUrl}
        placeholder="https://auth.exemplo.com/oauth/token"
      />
      <Campo rotulo="Client ID" valor={clientId} onChange={setClientId} />
      <Campo
        rotulo="Client Secret"
        valor={clientSecret}
        onChange={setClientSecret}
        segredo
      />
      <Campo rotulo="Scope" valor={scope} onChange={setScope} />

      {grant === "authorization_code" && (
        <>
          <Campo rotulo="Code" valor={code} onChange={setCode} segredo />
          <Campo
            rotulo="Redirect URI"
            valor={redirectUri}
            onChange={setRedirectUri}
          />
        </>
      )}

      {grant === "password" && (
        <>
          <Campo rotulo="Usuario" valor={usuario} onChange={setUsuario} />
          <Campo rotulo="Senha" valor={senha} onChange={setSenha} segredo />
        </>
      )}

      {grant === "refresh_token" && (
        <Campo
          rotulo="Refresh Token"
          valor={refreshToken}
          onChange={setRefreshToken}
          segredo
        />
      )}

      <div style={estilos.linha}>
        <label style={estilos.label} htmlFor="oauth-clientauth">
          Auth do cliente
        </label>
        <select
          id="oauth-clientauth"
          aria-label="Modo de auth do cliente"
          value={clientAuth}
          onChange={(e) => setClientAuth(e.target.value as "body" | "basic")}
          style={estilos.select}
        >
          <option value="body">No corpo (client_secret_post)</option>
          <option value="basic">Header Basic (client_secret_basic)</option>
        </select>
      </div>

      <div style={estilos.linha}>
        <button
          type="button"
          onClick={obterToken}
          disabled={carregando || tokenUrl.trim() === ""}
          style={estilos.botao}
        >
          {carregando ? "Obtendo..." : "Obter token"}
        </button>
      </div>

      {auth.token && (
        <Campo
          rotulo="Access Token"
          valor={auth.token}
          onChange={(v) => patch({ token: v })}
          segredo
        />
      )}

      {ok && (
        <p role="status" style={estilos.ok}>
          {ok}
        </p>
      )}
      {erro && (
        <p role="alert" style={estilos.erro}>
          Falha: {erro}
        </p>
      )}
    </div>
  );
}

// ---- Campo generico -------------------------------------------------------

function Campo(props: {
  rotulo: string;
  valor: string;
  onChange: (v: string) => void;
  segredo?: boolean;
  placeholder?: string;
}) {
  const { rotulo, valor, onChange, segredo, placeholder } = props;
  return (
    <div style={estilos.linha}>
      <label style={estilos.label}>{rotulo}</label>
      <input
        type={segredo ? "password" : "text"}
        aria-label={rotulo}
        value={valor}
        placeholder={placeholder}
        spellCheck={false}
        autoComplete="off"
        onChange={(e) => onChange(e.target.value)}
        style={estilos.input}
      />
    </div>
  );
}

// ---- Estilos (tema escuro coerente com o resto) ---------------------------

const estilos: Record<string, CSSProperties> = {
  container: {
    display: "flex",
    flexDirection: "column",
    gap: "0.6rem",
    width: "100%",
  },
  barra: {
    display: "flex",
    alignItems: "center",
    gap: "0.6rem",
  },
  campos: {
    display: "flex",
    flexDirection: "column",
    gap: "0.5rem",
  },
  linha: {
    display: "flex",
    alignItems: "center",
    gap: "0.6rem",
  },
  label: {
    color: "#9aa0a6",
    fontSize: "0.85rem",
    minWidth: "110px",
  },
  select: {
    background: "#1e1e1e",
    color: "#e0e0e0",
    border: "1px solid #3a3a3a",
    borderRadius: "4px",
    padding: "0.3rem 0.5rem",
    cursor: "pointer",
  },
  input: {
    flex: 1,
    background: "#1e1e1e",
    color: "#e0e0e0",
    border: "1px solid #3a3a3a",
    borderRadius: "4px",
    padding: "0.35rem 0.5rem",
    fontFamily: "monospace",
    fontSize: "0.85rem",
  },
  botao: {
    background: "#2a2a2a",
    color: "#e0e0e0",
    border: "1px solid #3a3a3a",
    borderRadius: "4px",
    padding: "0.4rem 0.9rem",
    fontSize: "0.85rem",
    cursor: "pointer",
  },
  vazio: {
    color: "#9aa0a6",
    fontSize: "0.85rem",
    fontStyle: "italic",
  },
  ok: {
    color: "#34d399",
    fontSize: "0.82rem",
  },
  erro: {
    color: "#f87171",
    fontSize: "0.82rem",
  },
};

export default AuthTab;
