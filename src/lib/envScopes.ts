// F9 — Construcao dos escopos de variaveis (LOGICA PURA, alvo de mutation).
//
// Monta o `VarScopes` (contrato compartilhado do M2) a partir do estado do
// envStore para a colecao ativa, e oferece helpers de merge/lookup que a F10
// (interpolacao) reusa. A precedencia (do mais forte ao mais fraco) e:
//   runtime > env (ambiente ativo) > collection > global
//
// Variaveis com `enabled === false` NAO contam na resolucao (sao ignoradas).
// O valor de variaveis `secret` e tratado como qualquer outro aqui (a logica de
// mascaramento e da UI, nao da resolucao).

/** Uma variavel reutilizavel. Espelho do `Variable` do backend (env + global). */
export interface Variable {
  name: string;
  value: string;
  enabled: boolean;
  secret: boolean;
}

/** Um ambiente nomeado com suas variaveis. Espelho do `Environment` do backend. */
export interface Environment {
  name: string;
  variables: Variable[];
}

/**
 * Escopos de resolucao, do mais forte (runtime) ao mais fraco (global).
 * Contrato compartilhado do M2 — a F10 consome exatamente esta forma.
 */
export interface VarScopes {
  /** Setado por scripts no futuro (M3); por ora {}. */
  runtime: Record<string, string>;
  /** Variaveis do environment ativo da colecao. */
  env: Variable[];
  /** Variaveis da colecao (collection.yml campo vars). */
  collection: Variable[];
  /** Variaveis globais do app (~/.config/ruan/globals.yml). */
  global: Variable[];
}

/**
 * Estado minimo (subset do envStore) necessario para construir os escopos de
 * UMA colecao. Mantido enxuto para a funcao pura nao depender do store inteiro.
 */
export interface EstadoColecaoEnv {
  environments: Environment[];
  /** Nome do environment ativo (null = nenhum ativo). */
  activeEnvName: string | null;
  collectionVars: Variable[];
  globalVars: Variable[];
  /** Variaveis de runtime (setadas por scripts no M3). */
  runtimeVars: Record<string, string>;
}

/** Acha o environment ativo pelo nome (null/ausente -> undefined). */
export function acharEnvAtivo(
  environments: Environment[],
  activeEnvName: string | null,
): Environment | undefined {
  if (activeEnvName === null) return undefined;
  return environments.find((e) => e.name === activeEnvName);
}

/**
 * Monta o `VarScopes` a partir do estado de uma colecao. As variaveis do
 * environment ativo entram em `env`; se nao houver ativo, `env` fica vazio.
 * NAO filtra `enabled` aqui — o filtro acontece no lookup (`buscarVar`),
 * para que a F10 possa decidir o comportamento; mantem os arrays intactos.
 */
export function construirScopes(estado: EstadoColecaoEnv): VarScopes {
  const envAtivo = acharEnvAtivo(estado.environments, estado.activeEnvName);
  return {
    runtime: { ...estado.runtimeVars },
    env: envAtivo ? [...envAtivo.variables] : [],
    collection: [...estado.collectionVars],
    global: [...estado.globalVars],
  };
}

/**
 * Busca o valor de uma variavel habilitada num array, pela 1a ocorrencia.
 * Variaveis com `enabled === false` sao ignoradas. Retorna `undefined` se nao
 * achar nenhuma habilitada com o nome dado.
 */
export function buscarVar(
  vars: Variable[],
  nome: string,
): string | undefined {
  for (const v of vars) {
    if (v.enabled && v.name === nome) {
      return v.value;
    }
  }
  return undefined;
}

/**
 * Resolve uma variavel pelo nome respeitando a precedencia
 * runtime > env > collection > global. Retorna `undefined` se nao houver
 * nenhuma fonte habilitada com esse nome. Helper central que a F10 reusa para
 * substituir cada `{{nome}}`.
 */
export function resolverVar(
  scopes: VarScopes,
  nome: string,
): string | undefined {
  // runtime e o mais forte: chave presente vence (mesmo string vazia).
  if (Object.prototype.hasOwnProperty.call(scopes.runtime, nome)) {
    return scopes.runtime[nome];
  }
  const noEnv = buscarVar(scopes.env, nome);
  if (noEnv !== undefined) return noEnv;
  const naColecao = buscarVar(scopes.collection, nome);
  if (naColecao !== undefined) return naColecao;
  return buscarVar(scopes.global, nome);
}
