// F10 — Logica PURA do realce de `{{var}}` em campos (alvo de mutation).
//
// Tokeniza um texto em segmentos literais e tokens `{{var}}`, e calcula a "dica"
// (resolvido/faltando) de cada token segundo os escopos. O componente VarField
// so renderiza estes segmentos; toda a decisao fica aqui, testavel.
//
// SEGURANCA: para variaveis `secret`, o valor resolvido NAO e exposto na dica
// (`mostrarValor=false`, `valor=undefined`); a UI mostra apenas que resolveu,
// nunca o segredo. Tokens nao resolvidos expoem so o NOME (que ja esta no texto).

import type { VarScopes, Variable } from "./envScopes";
import { resolverVar } from "./envScopes";

/** Mesma regex de `interpolation.ts`: miolo sem chaves, delimitado por {{ }}. */
const RE_TEMPLATE = /\{\{([^{}]*)\}\}/g;

/** Segmento de texto puro (sem variavel). */
export interface SegmentoTexto {
  tipo: "texto";
  conteudo: string;
}

/** Segmento que e um token `{{nome}}` com sua dica de resolucao. */
export interface SegmentoVar {
  tipo: "var";
  /** Texto cru do token, ex: "{{ base }}" (preserva espacos do original). */
  bruto: string;
  /** Nome ja "trimado" da variavel. */
  nome: string;
  /** True se a variavel resolveu em algum escopo habilitado. */
  resolvido: boolean;
  /** True se a fonte que resolveu e `secret` (valor nao deve ser exibido). */
  secret: boolean;
  /**
   * Valor resolvido para exibir na dica. `undefined` quando nao resolveu OU
   * quando e secret (nunca expoe segredo).
   */
  valor?: string;
}

export type Segmento = SegmentoTexto | SegmentoVar;

/**
 * Verifica se a variavel resolvida e `secret` em algum dos escopos (busca a 1a
 * ocorrencia habilitada, na mesma precedencia da resolucao). Usada so para
 * decidir mascaramento da dica; nao altera o valor enviado.
 */
function ehSecret(scopes: VarScopes, nome: string): boolean {
  // runtime nunca e secret (vem de scripts; sem flag) — checa as listas.
  const ordem: Variable[][] = [scopes.env, scopes.collection, scopes.global];
  // runtime tem prioridade: se a chave existe la, nao e secret.
  if (Object.prototype.hasOwnProperty.call(scopes.runtime, nome)) return false;
  for (const lista of ordem) {
    const achou = lista.find((v) => v.enabled && v.name === nome);
    if (achou) return achou.secret;
  }
  return false;
}

/**
 * Quebra `texto` em segmentos literais e tokens `{{var}}`, anotando a dica de
 * cada token. PURA. Tokens com nome vazio (`{{}}`/`{{  }}`) sao tratados como
 * texto literal (nao sao variaveis).
 */
export function segmentar(texto: string, scopes: VarScopes): Segmento[] {
  const segmentos: Segmento[] = [];
  let ultimo = 0;
  // Regex com flag global: precisa de instancia local para lastIndex isolado.
  const re = new RegExp(RE_TEMPLATE.source, "g");
  let m: RegExpExecArray | null;

  while ((m = re.exec(texto)) !== null) {
    const bruto = m[0];
    const nome = m[1].trim();

    // `{{}}` sem nome: nao e variavel -> deixa fluir como texto literal.
    if (nome === "") continue;

    // Empurra o trecho de texto antes do token.
    if (m.index > ultimo) {
      segmentos.push({ tipo: "texto", conteudo: texto.slice(ultimo, m.index) });
    }

    const valorResolvido = resolverVar(scopes, nome);
    const resolvido = valorResolvido !== undefined;
    const secret = resolvido && ehSecret(scopes, nome);

    segmentos.push({
      tipo: "var",
      bruto,
      nome,
      resolvido,
      secret,
      // Nunca expoe valor de secret na dica.
      valor: resolvido && !secret ? valorResolvido : undefined,
    });

    ultimo = m.index + bruto.length;
  }

  // Resto do texto apos o ultimo token.
  if (ultimo < texto.length) {
    segmentos.push({ tipo: "texto", conteudo: texto.slice(ultimo) });
  }

  return segmentos;
}

/**
 * Monta o texto da tooltip de um token de variavel. PURA.
 * Nunca inclui valor de secret (mostra apenas "(secreto)").
 */
export function dicaDoToken(seg: SegmentoVar): string {
  if (!seg.resolvido) return `${seg.nome}: nao resolvida`;
  if (seg.secret) return `${seg.nome}: (secreto)`;
  return `${seg.nome} = ${seg.valor ?? ""}`;
}
