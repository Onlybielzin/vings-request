// Store Zustand das colecoes abertas. Stub minimo do F1; F2 adiciona o CRUD no
// nivel do app (criar/abrir/fechar) e a persistencia da LISTA de colecoes
// abertas entre sessoes. Estado indexado pelo caminho da colecao.

import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { Collection } from "../lib/types";
import { normalizarCollection } from "../lib/types";
import * as ipc from "../lib/ipc";

/** Abre uma colecao via IPC e normaliza a arvore (defaults p/ campos omitidos). */
async function carregarColecao(path: string): Promise<Collection> {
  return normalizarCollection(await ipc.openCollection(path));
}

// ---- Wrappers IPC proprios da F2 ----
// Mantidos aqui (em vez de em lib/ipc.ts) porque ipc.ts pertence a outra onda.
// Cada um corresponde a um #[tauri::command] registrado na fase de Integracao.

/** Cria uma colecao nova em `parent` com `name`. Retorna a colecao carregada. */
export function ipcCreateCollection(
  parent: string,
  name: string,
): Promise<Collection> {
  return invoke<Collection>("create_collection", { parent, name });
}

/** Le a lista persistida de colecoes abertas (caminhos absolutos). */
export function ipcLoadOpenCollections(): Promise<string[]> {
  return invoke<string[]>("load_open_collections_cmd");
}

/** Persiste a lista de colecoes abertas. */
export function ipcSaveOpenCollections(paths: string[]): Promise<void> {
  return invoke<void>("save_open_collections_cmd", { paths });
}

interface CollectionsState {
  /** Colecoes abertas, indexadas pelo caminho do diretorio. */
  collections: Record<string, Collection>;
  /** Ordem de exibicao das colecoes abertas (caminhos). Fonte da persistencia. */
  ordem: string[];
  /** Caminho da colecao ativa, se houver. */
  activePath: string | null;
  loading: boolean;
  error: string | null;

  /** Abre uma colecao do disco e a coloca como ativa. */
  openCollection: (path: string) => Promise<void>;
  /** Recarrega uma colecao ja aberta a partir do disco. */
  reloadCollection: (path: string) => Promise<void>;
  /** Fecha uma colecao aberta (remove do estado; NAO apaga do disco). */
  closeCollection: (path: string) => void;
  /** Define a colecao ativa. */
  setActive: (path: string | null) => void;

  // ---- F2: CRUD no nivel do app ----
  /** Abre um seletor de diretorio e abre a colecao escolhida. */
  abrirColecao: () => Promise<void>;
  /**
   * Cria uma colecao nova: pede o diretorio-pai via dialog, cria no disco com
   * `nome` e abre a colecao resultante. Sem dialog (testes), passe `parentDir`.
   */
  criarColecao: (nome: string, parentDir?: string) => Promise<void>;
  /** Fecha uma colecao (alias semantico de closeCollection). */
  fecharColecao: (path: string) => void;
  /** Restaura a lista de colecoes persistida e reabre cada uma do disco. */
  restaurarColecoes: () => Promise<void>;
}

/** Adiciona `path` ao fim de `ordem` se ainda nao estiver presente. */
function comPath(ordem: string[], path: string): string[] {
  return ordem.includes(path) ? ordem : [...ordem, path];
}

/** Remove `path` de `ordem`. */
function semPath(ordem: string[], path: string): string[] {
  return ordem.filter((p) => p !== path);
}

export const useCollectionsStore = create<CollectionsState>((set, get) => ({
  collections: {},
  ordem: [],
  activePath: null,
  loading: false,
  error: null,

  openCollection: async (path) => {
    set({ loading: true, error: null });
    try {
      const collection = await carregarColecao(path);
      set((state) => ({
        collections: { ...state.collections, [path]: collection },
        ordem: comPath(state.ordem, path),
        activePath: path,
        loading: false,
      }));
      void persistir(get);
    } catch (e) {
      set({ loading: false, error: String(e) });
    }
  },

  reloadCollection: async (path) => {
    try {
      const collection = await carregarColecao(path);
      set((state) => ({
        collections: { ...state.collections, [path]: collection },
      }));
    } catch (e) {
      set({ error: String(e) });
    }
  },

  closeCollection: (path) => {
    set((state) => {
      const next = { ...state.collections };
      delete next[path];
      const activePath = state.activePath === path ? null : state.activePath;
      return {
        collections: next,
        ordem: semPath(state.ordem, path),
        activePath,
      };
    });
    void persistir(get);
  },

  setActive: (path) => {
    if (path !== null && !get().collections[path]) return;
    set({ activePath: path });
  },

  // ---- F2 ----

  abrirColecao: async () => {
    set({ error: null });
    let escolhido: string | null;
    try {
      const sel = await openDialog({ directory: true, multiple: false });
      // O dialog devolve string | string[] | null conforme a versao/opcoes.
      escolhido = Array.isArray(sel) ? (sel[0] ?? null) : sel;
    } catch (e) {
      set({ error: String(e) });
      return;
    }
    if (!escolhido) return; // usuario cancelou
    await get().openCollection(escolhido);
  },

  criarColecao: async (nome, parentDir) => {
    set({ error: null });
    let parent = parentDir ?? null;
    if (parent === null) {
      try {
        const sel = await openDialog({ directory: true, multiple: false });
        parent = Array.isArray(sel) ? (sel[0] ?? null) : sel;
      } catch (e) {
        set({ error: String(e) });
        return;
      }
    }
    if (!parent) return; // usuario cancelou
    set({ loading: true });
    try {
      const collection = normalizarCollection(await ipcCreateCollection(parent, nome));
      // O backend cria <parent>/<slug(nome)>/. Como nao recebemos o caminho
      // exato de volta, reabrimos pela API padrao a partir do caminho slugado.
      const path = juntarCaminho(parent, slugFront(nome));
      set((state) => ({
        collections: { ...state.collections, [path]: collection },
        ordem: comPath(state.ordem, path),
        activePath: path,
        loading: false,
      }));
      void persistir(get);
    } catch (e) {
      set({ loading: false, error: String(e) });
    }
  },

  fecharColecao: (path) => {
    get().closeCollection(path);
  },

  restaurarColecoes: async () => {
    let paths: string[] = [];
    try {
      paths = await ipcLoadOpenCollections();
    } catch (e) {
      set({ error: String(e) });
      return;
    }
    // Reabre cada uma; falha numa colecao (ex.: pasta movida/apagada) nao
    // impede as demais. openCollection ja persiste a lista resultante, entao
    // colecoes que nao carregam somem da persistencia naturalmente.
    for (const p of paths) {
      try {
        const collection = await carregarColecao(p);
        set((state) => ({
          collections: { ...state.collections, [p]: collection },
          ordem: comPath(state.ordem, p),
        }));
      } catch {
        // ignora colecao que nao carrega
      }
    }
    // Reescreve a persistencia com so o que carregou com sucesso.
    void persistir(get);
    // Ativa a primeira, se houver.
    const ordem = get().ordem;
    if (ordem.length > 0 && get().activePath === null) {
      set({ activePath: ordem[0] });
    }
  },
}));

/** Persiste a ordem atual via IPC. Best-effort: erro nao quebra a UI. */
function persistir(get: () => CollectionsState): Promise<void> {
  return ipcSaveOpenCollections(get().ordem).catch(() => {
    // best-effort; nao propaga
  });
}

/** Junta diretorio-pai e nome num caminho, tolerando barra final no pai. */
export function juntarCaminho(parent: string, nome: string): string {
  const base = parent.endsWith("/") ? parent.slice(0, -1) : parent;
  return `${base}/${nome}`;
}

/**
 * Espelho FRONT do `slug_seguro` do backend (apenas pra prever o nome da pasta
 * criada). Minusculas, ascii, [a-z0-9-], hifens colapsados e aparados. NAO e a
 * fonte de verdade da seguranca (essa e o Rust); aqui so reconstruimos o caminho.
 */
export function slugFront(nome: string): string {
  return nome
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "") // remove diacriticos combinantes
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-") // nao-alfanumerico vira hifen
    .replace(/^-+|-+$/g, ""); // apara hifens das pontas
}
