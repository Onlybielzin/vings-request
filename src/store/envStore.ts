// F9 — Store Zustand de environments e variaveis (3 escopos: env ativo, colecao,
// global). Indexado pelo caminho da colecao, pois varias colecoes podem estar
// abertas ao mesmo tempo (cada uma com seus environments e env ativo proprio).
//
// As variaveis GLOBAIS sao do app inteiro (nao por colecao), mas as guardamos
// no estado de cada colecao para que `construirScopes` monte o VarScopes da
// colecao ativa num lugar so. A persistencia global usa comandos separados.
//
// Persistencia:
//   - environments  -> list_environments / save_environment_cmd / delete_environment_cmd
//   - global vars   -> load_global_vars_cmd / save_global_vars_cmd
//   - collection vars: por ora apenas em memoria (vem do collection.yml campo
//     `vars`; a gravacao no collection.yml e de outra onda — aqui expomos a
//     edicao em memoria que `construirScopes` consome).
//
// Variaveis secret: este store nunca loga valores; o mascaramento e da UI.

import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type {
  Environment,
  Variable,
  VarScopes,
  EstadoColecaoEnv,
} from "../lib/envScopes";
import { construirScopes } from "../lib/envScopes";

// ---- Wrappers IPC (cada um e um #[tauri::command] registrado na Integracao) ----

/** Lista os environments de uma colecao (do disco). */
export function ipcListEnvironments(
  collectionDir: string,
): Promise<Environment[]> {
  return invoke<Environment[]>("list_environments", { collectionDir });
}

/** Grava (cria/atualiza) um environment na colecao. */
export function ipcSaveEnvironment(
  collectionDir: string,
  env: Environment,
): Promise<void> {
  return invoke<void>("save_environment_cmd", { collectionDir, env });
}

/** Remove um environment pelo nome. */
export function ipcDeleteEnvironment(
  collectionDir: string,
  name: string,
): Promise<void> {
  return invoke<void>("delete_environment_cmd", { collectionDir, name });
}

/** Le as variaveis globais do app. */
export function ipcLoadGlobalVars(): Promise<Variable[]> {
  return invoke<Variable[]>("load_global_vars_cmd");
}

/** Persiste as variaveis globais do app. */
export function ipcSaveGlobalVars(vars: Variable[]): Promise<void> {
  return invoke<void>("save_global_vars_cmd", { vars });
}

/** Estado por colecao. Espelha o subset que `construirScopes` consome. */
export interface ColecaoEnvState {
  environments: Environment[];
  activeEnvName: string | null;
  collectionVars: Variable[];
  globalVars: Variable[];
  runtimeVars: Record<string, string>;
}

/** Estado inicial vazio de uma colecao recem-vista. */
export function estadoColecaoVazio(): ColecaoEnvState {
  return {
    environments: [],
    activeEnvName: null,
    collectionVars: [],
    globalVars: [],
    runtimeVars: {},
  };
}

interface EnvStoreState {
  /** Estado de env indexado pelo caminho da colecao. */
  porColecao: Record<string, ColecaoEnvState>;
  error: string | null;

  /** Garante que existe entrada para a colecao (sem sobrescrever a existente). */
  garantirColecao: (path: string) => ColecaoEnvState;
  /** Carrega environments (do disco) + globais (do app) para a colecao. */
  carregar: (path: string) => Promise<void>;
  /** Cria/atualiza um environment e persiste. */
  salvarEnvironment: (path: string, env: Environment) => Promise<void>;
  /** Remove um environment e persiste; limpa o ativo se for o removido. */
  excluirEnvironment: (path: string, name: string) => Promise<void>;
  /** Define o environment ativo da colecao (null = nenhum). */
  setActiveEnv: (path: string, name: string | null) => void;
  /** Substitui as variaveis da colecao (em memoria). */
  setCollectionVars: (path: string, vars: Variable[]) => void;
  /** Substitui e PERSISTE as variaveis globais (refletidas em todas as colecoes). */
  setGlobalVars: (path: string, vars: Variable[]) => Promise<void>;
  /** Define as variaveis de runtime (M3 — scripts). */
  setRuntimeVars: (path: string, vars: Record<string, string>) => void;
  /** Monta o VarScopes da colecao (delega a `construirScopes`, puro). */
  scopesDe: (path: string | null) => VarScopes;
}

/** Cria uma variavel vazia (habilitada, nao-secreta) para a UI. */
export function novaVariavel(): Variable {
  return { name: "", value: "", enabled: true, secret: false };
}

/** Cria um environment vazio com o nome dado. */
export function novoEnvironment(name: string): Environment {
  return { name, variables: [] };
}

export const useEnvStore = create<EnvStoreState>((set, get) => ({
  porColecao: {},
  error: null,

  garantirColecao: (path) => {
    const existente = get().porColecao[path];
    if (existente) return existente;
    const novo = estadoColecaoVazio();
    set((s) => ({ porColecao: { ...s.porColecao, [path]: novo } }));
    return novo;
  },

  carregar: async (path) => {
    set({ error: null });
    try {
      const [environments, globalVars] = await Promise.all([
        ipcListEnvironments(path),
        ipcLoadGlobalVars(),
      ]);
      set((s) => {
        const atual = s.porColecao[path] ?? estadoColecaoVazio();
        // Mantem o env ativo se ainda existir apos o reload.
        const aindaExiste =
          atual.activeEnvName !== null &&
          environments.some((e) => e.name === atual.activeEnvName);
        return {
          porColecao: {
            ...s.porColecao,
            [path]: {
              ...atual,
              environments,
              globalVars,
              activeEnvName: aindaExiste ? atual.activeEnvName : null,
            },
          },
        };
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  salvarEnvironment: async (path, env) => {
    set({ error: null });
    try {
      await ipcSaveEnvironment(path, env);
    } catch (e) {
      set({ error: String(e) });
      return;
    }
    set((s) => {
      const atual = s.porColecao[path] ?? estadoColecaoVazio();
      const outros = atual.environments.filter((e) => e.name !== env.name);
      const environments = [...outros, env].sort((a, b) =>
        a.name.localeCompare(b.name),
      );
      return {
        porColecao: {
          ...s.porColecao,
          [path]: { ...atual, environments },
        },
      };
    });
  },

  excluirEnvironment: async (path, name) => {
    set({ error: null });
    try {
      await ipcDeleteEnvironment(path, name);
    } catch (e) {
      set({ error: String(e) });
      return;
    }
    set((s) => {
      const atual = s.porColecao[path] ?? estadoColecaoVazio();
      const environments = atual.environments.filter((e) => e.name !== name);
      const activeEnvName =
        atual.activeEnvName === name ? null : atual.activeEnvName;
      return {
        porColecao: {
          ...s.porColecao,
          [path]: { ...atual, environments, activeEnvName },
        },
      };
    });
  },

  setActiveEnv: (path, name) => {
    set((s) => {
      const atual = s.porColecao[path] ?? estadoColecaoVazio();
      return {
        porColecao: {
          ...s.porColecao,
          [path]: { ...atual, activeEnvName: name },
        },
      };
    });
  },

  setCollectionVars: (path, vars) => {
    set((s) => {
      const atual = s.porColecao[path] ?? estadoColecaoVazio();
      return {
        porColecao: {
          ...s.porColecao,
          [path]: { ...atual, collectionVars: vars },
        },
      };
    });
  },

  setGlobalVars: async (path, vars) => {
    set({ error: null });
    try {
      await ipcSaveGlobalVars(vars);
    } catch (e) {
      set({ error: String(e) });
      return;
    }
    // Globais valem para o app inteiro: refletir em TODAS as colecoes em memoria.
    set((s) => {
      const porColecao: Record<string, ColecaoEnvState> = {};
      for (const [p, st] of Object.entries(s.porColecao)) {
        porColecao[p] = { ...st, globalVars: vars };
      }
      // Garante a colecao corrente mesmo que ainda nao estivesse no mapa.
      if (!porColecao[path]) {
        porColecao[path] = { ...estadoColecaoVazio(), globalVars: vars };
      }
      return { porColecao };
    });
  },

  setRuntimeVars: (path, vars) => {
    set((s) => {
      const atual = s.porColecao[path] ?? estadoColecaoVazio();
      return {
        porColecao: {
          ...s.porColecao,
          [path]: { ...atual, runtimeVars: vars },
        },
      };
    });
  },

  scopesDe: (path) => {
    const vazio: EstadoColecaoEnv = estadoColecaoVazio();
    if (path === null) return construirScopes(vazio);
    const atual = get().porColecao[path] ?? vazio;
    return construirScopes(atual);
  },
}));
