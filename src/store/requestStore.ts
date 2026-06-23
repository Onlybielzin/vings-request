// Store Zustand da request em edicao + envio (F4). Os paineis das features
// F5 (params/headers), F6 (body) e F7 (auth) plugam aqui via `atualizarRequest`
// (patch generico), sem precisar editar este store.

import { create } from "zustand";
import type { RequestItem } from "../lib/types";
import { novaRequest, normalizarRequest } from "../lib/types";
import type { ResponseData } from "../lib/http-types";
import { requestDataDeItem, mensagemDeErro } from "../lib/http-types";
import { sendRequest } from "../lib/sendClient";
import { interpolarRequest } from "../lib/interpolation";
import {
  resolverAuthEfetiva,
  aplicarAuth,
  mesclarSemSobrescrever,
} from "../lib/auth";
import { useEnvStore } from "./envStore";
import { useCollectionsStore } from "./collectionsStore";

interface RequestState {
  /** Request atualmente em edicao no builder. */
  request: RequestItem;
  /** Ultima resposta recebida (null antes do primeiro envio). */
  response: ResponseData | null;
  /** True enquanto um envio esta em andamento. */
  loading: boolean;
  /** Mensagem do ultimo erro de envio (null se ok). */
  error: string | null;
  /**
   * Nomes de variaveis `{{var}}` que NAO resolveram no ultimo envio (so NOMES,
   * nunca valores — vars secret nao vazam aqui). A UI mostra como aviso nao
   * bloqueante. Vazio = tudo resolvido.
   */
  avisoVars: string[];

  /**
   * Aplica um patch parcial na request atual. Generico de proposito: qualquer
   * painel (metodo, url, headers, params, body, auth...) usa isto.
   */
  atualizarRequest: (patch: Partial<RequestItem>) => void;
  /** Substitui a request inteira (ex: ao selecionar outra na arvore). */
  setRequest: (request: RequestItem) => void;
  /** Dispara a request atual e guarda resposta/erro/loading. */
  enviar: () => Promise<void>;
  /** Limpa a resposta/erro (ex: ao trocar de request). */
  limparResposta: () => void;
}

export const useRequestStore = create<RequestState>((set, get) => ({
  request: novaRequest("Nova Request"),
  response: null,
  loading: false,
  error: null,
  avisoVars: [],

  atualizarRequest: (patch) => {
    set((state) => ({ request: { ...state.request, ...patch } }));
  },

  setRequest: (request) => {
    // Normaliza na costura IPC: a request vinda da arvore pode ter headers/params/
    // body.form omitidos pelo serde do backend. Sem isso, os paineis quebram ao
    // iterar (undefined.map) e a tela fica preta ao selecionar uma request.
    set({
      request: normalizarRequest(request),
      response: null,
      error: null,
      avisoVars: [],
    });
  },

  enviar: async () => {
    // Evita envios concorrentes do mesmo store.
    if (get().loading) return;
    set({ loading: true, error: null });
    try {
      // Monta os escopos da colecao ativa e interpola `{{var}}` ANTES do envio.
      // O Rust so executa HTTP; a resolucao de variaveis e do front (decisao M2).
      const collectionsState = useCollectionsStore.getState();
      const activePath = collectionsState.activePath;
      const scopes = useEnvStore.getState().scopesDe(activePath);
      const bruta = requestDataDeItem(get().request);
      const { req, faltando } = interpolarRequest(bruta, scopes);
      // `faltando` NAO bloqueia o envio: guarda apenas NOMES para aviso na UI.

      // F11 — resolve a auth EFETIVA (heranca request -> pasta -> colecao) e
      // mescla os headers/query produzidos. A auth da colecao ativa e o topo da
      // cadeia de `mode: 'inherit'`. A auth de PASTA depende de um breadcrumb da
      // request ativa que este store ainda nao rastreia; quando disponivel, a
      // Integracao pode passar `folderAuth` em vez de undefined. Os campos de
      // auth sao interpolados em `aplicarAuth` (reusa a F10) antes de virar
      // headers/query. A mescla NAO sobrescreve o que o usuario definiu na mao.
      const colecaoAtiva = activePath
        ? collectionsState.collections[activePath]
        : undefined;
      const authEfetiva = resolverAuthEfetiva(
        get().request.auth,
        undefined,
        colecaoAtiva?.auth,
      );
      const aplicada = aplicarAuth(authEfetiva, scopes);
      // Projeta os pares de auth no shape enxuto KeyVal do envio (name/value/
      // enabled), descartando `description` que o envio nao usa.
      const authHeaders = aplicada.headers.map((h) => ({
        name: h.name,
        value: h.value,
        enabled: h.enabled,
      }));
      const authQuery = aplicada.query.map((q) => ({
        name: q.name,
        value: q.value,
        enabled: q.enabled,
      }));
      const reqComAuth = {
        ...req,
        // Nao sobrescreve headers/params que o usuario ja definiu na mao.
        headers: mesclarSemSobrescrever(req.headers, authHeaders, true),
        params: mesclarSemSobrescrever(req.params, authQuery, false),
      };

      const response = await sendRequest(reqComAuth);
      set({ response, loading: false, error: null, avisoVars: faltando });
    } catch (e) {
      set({ loading: false, error: mensagemDeErro(e) });
    }
  },

  limparResposta: () => {
    set({ response: null, error: null, avisoVars: [] });
  },
}));
