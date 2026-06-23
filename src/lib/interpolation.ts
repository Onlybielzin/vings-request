// F10 — Interpolacao de variaveis `{{var}}` (LOGICA PURA, alvo de mutation).
//
// Substitui ocorrencias de `{{nome}}` em textos e na request inteira pelos
// valores resolvidos dos escopos (contrato M2), respeitando a precedencia
// runtime > env > collection > global (delegada a `resolverVar` da F9).
//
// Regras (do CONTRATO):
//   - chaves podem ter espacos internos: `{{ nome }}` -> trim para "nome".
//   - delimitadores desbalanceados (`{{` sem `}}`, ou `}}` solto) ficam LITERAIS.
//   - multiplas ocorrencias no mesmo texto sao todas substituidas.
//   - UMA passada so: se o valor de uma var contem `{{...}}`, NAO reinterpola
//     (evita loop e injecao de template via valor de variavel).
//   - variaveis desabilitadas (enabled=false) NAO contam (cuidado da F9).
//   - nao-resolvidas ficam literais (mantem o `{{nome}}` no texto) e seus NOMES
//     entram em `faltando` (sem repeticao, na ordem de 1a ocorrencia).
//
// SEGURANCA: nada de valores aqui vaza para `faltando` — so NOMES de variaveis
// que NAO resolveram. Valores resolvidos (inclusive de vars secret) entram
// apenas no texto final destinado ao envio, nunca em avisos/logs.

import type { VarScopes } from "./envScopes";
import { resolverVar } from "./envScopes";
import type { RequestData, KeyVal, RequestBody } from "./http-types";

/** Resultado de uma interpolacao de texto. */
export interface ResultadoTemplate {
  /** Texto com as variaveis resolvidas substituidas. */
  valor: string;
  /** Nomes (unicos, em ordem de 1a aparicao) que NAO resolveram. */
  faltando: string[];
}

// Captura `{{ ... }}` onde o miolo nao contem `{` nem `}` (evita casar
// delimitadores aninhados/desbalanceados como `{{{x}}`). O miolo e qualquer
// sequencia sem chaves; o trim e aplicado depois para tolerar `{{ x }}`.
const RE_TEMPLATE = /\{\{([^{}]*)\}\}/g;

/**
 * Resolve os `{{var}}` de um texto segundo os escopos. PURA.
 * Faz UMA passada (nao reinterpola valores). Tokens nao resolvidos ficam
 * literais e seus nomes vao para `faltando` (unicos, em ordem).
 */
export function resolverTemplate(
  texto: string,
  scopes: VarScopes,
): ResultadoTemplate {
  const faltando: string[] = [];
  if (texto === "") return { valor: "", faltando };

  const valor = texto.replace(RE_TEMPLATE, (original, miolo: string) => {
    const nome = miolo.trim();
    // `{{}}` ou `{{   }}` (nome vazio): nao e variavel valida -> literal.
    if (nome === "") return original;
    const resolvido = resolverVar(scopes, nome);
    if (resolvido === undefined) {
      if (!faltando.includes(nome)) faltando.push(nome);
      return original; // mantem o token literal
    }
    return resolvido; // UMA passada: valor entra cru, sem reinterpolar
  });

  return { valor, faltando };
}

/** Acumula os `faltando` de varios resultados num set ordenado (sem repetir). */
function acumular(destino: string[], novos: string[]): void {
  for (const n of novos) {
    if (!destino.includes(n)) destino.push(n);
  }
}

/** Interpola um KeyVal (name e value). Mantem `enabled`. PURA. */
function interpolarKeyVal(
  kv: KeyVal,
  scopes: VarScopes,
  faltando: string[],
): KeyVal {
  const rName = resolverTemplate(kv.name, scopes);
  const rValue = resolverTemplate(kv.value, scopes);
  acumular(faltando, rName.faltando);
  acumular(faltando, rValue.faltando);
  return { name: rName.valor, value: rValue.valor, enabled: kv.enabled };
}

/** Interpola o corpo (raw + values do form). PURA. */
function interpolarBody(
  body: RequestBody,
  scopes: VarScopes,
  faltando: string[],
): RequestBody {
  let raw = body.raw;
  if (raw !== undefined) {
    const r = resolverTemplate(raw, scopes);
    raw = r.valor;
    acumular(faltando, r.faltando);
  }
  const form = body.form.map((kv) => interpolarKeyVal(kv, scopes, faltando));
  return { ...body, raw, form };
}

/** Resultado da interpolacao de uma request inteira. */
export interface ResultadoRequest {
  req: RequestData;
  /** Nomes unicos (ordem de 1a aparicao) que NAO resolveram em todo o request. */
  faltando: string[];
}

/**
 * Interpola a request inteira: url, params (name/value), headers (name/value)
 * e body (raw + form values). Retorna nova RequestData (nao muta a entrada) e a
 * lista agregada de nomes faltando. PURA.
 *
 * Observacao: `timeoutMs` e numero, nao passa por interpolacao.
 */
export function interpolarRequest(
  req: RequestData,
  scopes: VarScopes,
): ResultadoRequest {
  const faltando: string[] = [];

  const rUrl = resolverTemplate(req.url, scopes);
  acumular(faltando, rUrl.faltando);

  const headers = req.headers.map((kv) =>
    interpolarKeyVal(kv, scopes, faltando),
  );
  const params = req.params.map((kv) => interpolarKeyVal(kv, scopes, faltando));
  const body = interpolarBody(req.body, scopes, faltando);

  return {
    req: {
      ...req,
      url: rUrl.valor,
      headers,
      params,
      body,
    },
    faltando,
  };
}
