import { describe, it, expect } from "vitest";
import type { VarScopes, Variable } from "./envScopes";
import { segmentar, dicaDoToken, type SegmentoVar } from "./useVarHint";

function v(
  name: string,
  value: string,
  enabled = true,
  secret = false,
): Variable {
  return { name, value, enabled, secret };
}

function scopes(p: Partial<VarScopes> = {}): VarScopes {
  return { runtime: {}, env: [], collection: [], global: [], ...p };
}

describe("segmentar", () => {
  it("texto puro vira um unico segmento de texto", () => {
    expect(segmentar("abc", scopes())).toEqual([
      { tipo: "texto", conteudo: "abc" },
    ]);
  });

  it("token resolvido (nao secret) traz valor e resolvido=true", () => {
    const s = scopes({ env: [v("base", "http://api")] });
    const segs = segmentar("{{base}}/x", s);
    expect(segs[0]).toEqual({
      tipo: "var",
      bruto: "{{base}}",
      nome: "base",
      resolvido: true,
      secret: false,
      valor: "http://api",
    });
    expect(segs[1]).toEqual({ tipo: "texto", conteudo: "/x" });
  });

  it("token faltando: resolvido=false e sem valor", () => {
    const segs = segmentar("{{x}}", scopes());
    const seg = segs[0] as SegmentoVar;
    expect(seg.resolvido).toBe(false);
    expect(seg.valor).toBeUndefined();
  });

  it("token secret resolvido NAO expoe valor", () => {
    const s = scopes({ env: [v("k", "segredo", true, true)] });
    const seg = segmentar("{{k}}", s)[0] as SegmentoVar;
    expect(seg.resolvido).toBe(true);
    expect(seg.secret).toBe(true);
    expect(seg.valor).toBeUndefined();
  });

  it("preserva espacos internos no bruto mas trima o nome", () => {
    const s = scopes({ env: [v("k", "V")] });
    const seg = segmentar("{{ k }}", s)[0] as SegmentoVar;
    expect(seg.bruto).toBe("{{ k }}");
    expect(seg.nome).toBe("k");
  });

  it("{{}} vazio nao vira var (fica texto)", () => {
    const segs = segmentar("a{{}}b", scopes());
    expect(segs.every((s) => s.tipo === "texto")).toBe(true);
  });

  it("runtime nunca e secret mesmo com mesmo nome em env secret", () => {
    const s = scopes({
      runtime: { k: "R" },
      env: [v("k", "segredo", true, true)],
    });
    const seg = segmentar("{{k}}", s)[0] as SegmentoVar;
    expect(seg.secret).toBe(false);
    expect(seg.valor).toBe("R");
  });

  it("multiplos tokens e textos intercalados", () => {
    const s = scopes({ env: [v("a", "A"), v("b", "B")] });
    const segs = segmentar("x{{a}}y{{b}}z", s);
    expect(segs.map((seg) => (seg.tipo === "texto" ? seg.conteudo : "*"))).toEqual(
      ["x", "*", "y", "*", "z"],
    );
  });

  it("var desabilitada conta como nao resolvida", () => {
    const s = scopes({ env: [v("k", "V", false)] });
    const seg = segmentar("{{k}}", s)[0] as SegmentoVar;
    expect(seg.resolvido).toBe(false);
  });

  it("token no inicio nao gera segmento de texto vazio antes", () => {
    // m.index === ultimo (0): NAO deve empurrar texto vazio.
    const s = scopes({ env: [v("a", "A")] });
    const segs = segmentar("{{a}}fim", s);
    expect(segs[0].tipo).toBe("var");
    expect(segs).toHaveLength(2);
    expect(segs[1]).toEqual({ tipo: "texto", conteudo: "fim" });
  });

  it("token no fim nao gera segmento de texto vazio depois", () => {
    // ultimo === texto.length: NAO deve empurrar texto vazio final.
    const s = scopes({ env: [v("a", "A")] });
    const segs = segmentar("ini{{a}}", s);
    expect(segs).toHaveLength(2);
    expect(segs[0]).toEqual({ tipo: "texto", conteudo: "ini" });
    expect(segs[1].tipo).toBe("var");
  });

  it("texto vazio devolve lista vazia (sem segmento de texto vazio)", () => {
    expect(segmentar("", scopes())).toEqual([]);
  });

  it("dois tokens colados nao geram texto vazio no meio", () => {
    const s = scopes({ env: [v("a", "A"), v("b", "B")] });
    const segs = segmentar("{{a}}{{b}}", s);
    expect(segs).toHaveLength(2);
    expect(segs.every((seg) => seg.tipo === "var")).toBe(true);
  });

  it("secret lido da mesma fonte que resolve (env secret vence collection nao-secret)", () => {
    const s = scopes({
      env: [v("k", "segredo", true, true)],
      collection: [v("k", "publico", true, false)],
    });
    const seg = segmentar("{{k}}", s)[0] as SegmentoVar;
    expect(seg.resolvido).toBe(true);
    expect(seg.secret).toBe(true);
    expect(seg.valor).toBeUndefined();
  });

  it("collection nao-secret resolve quando env tem a var desabilitada secret", () => {
    // a fonte habilitada que resolve e a collection (nao secret) -> expoe valor.
    const s = scopes({
      env: [v("k", "segredo", false, true)],
      collection: [v("k", "publico", true, false)],
    });
    const seg = segmentar("{{k}}", s)[0] as SegmentoVar;
    expect(seg.resolvido).toBe(true);
    expect(seg.secret).toBe(false);
    expect(seg.valor).toBe("publico");
  });

  it("ehSecret ignora ocorrencia desabilitada e le a 1a habilitada", () => {
    // primeira desabilitada (nao-secret), segunda habilitada secret -> secret=true.
    const s = scopes({
      env: [v("k", "a", false, false), v("k", "b", true, true)],
    });
    const seg = segmentar("{{k}}", s)[0] as SegmentoVar;
    expect(seg.secret).toBe(true);
  });

  it("global secret e mascarado", () => {
    const s = scopes({ global: [v("k", "g", true, true)] });
    const seg = segmentar("{{k}}", s)[0] as SegmentoVar;
    expect(seg.secret).toBe(true);
    expect(seg.valor).toBeUndefined();
  });

  it("nao resolvida nunca e marcada secret", () => {
    const seg = segmentar("{{x}}", scopes())[0] as SegmentoVar;
    expect(seg.resolvido).toBe(false);
    expect(seg.secret).toBe(false);
  });

  it("ehSecret casa pelo NOME, nao pega a 1a var habilitada qualquer", () => {
    // env tem uma var habilitada de OUTRO nome antes da var alvo. O mutante
    // `v.enabled && true` pegaria a 1a habilitada ("outra", nao-secret) e
    // retornaria secret=false; o correto casa "k" (secret=true).
    const s = scopes({
      env: [v("outra", "x", true, false), v("k", "s=", true, true)],
    });
    const seg = segmentar("{{k}}", s)[0] as SegmentoVar;
    expect(seg.secret).toBe(true);
    expect(seg.valor).toBeUndefined();
  });
});

describe("dicaDoToken", () => {
  it("nao resolvida", () => {
    expect(
      dicaDoToken({
        tipo: "var",
        bruto: "{{x}}",
        nome: "x",
        resolvido: false,
        secret: false,
      }),
    ).toBe("x: nao resolvida");
  });

  it("secret mostra (secreto) sem valor", () => {
    expect(
      dicaDoToken({
        tipo: "var",
        bruto: "{{k}}",
        nome: "k",
        resolvido: true,
        secret: true,
      }),
    ).toBe("k: (secreto)");
  });

  it("resolvida nao secret mostra valor", () => {
    expect(
      dicaDoToken({
        tipo: "var",
        bruto: "{{k}}",
        nome: "k",
        resolvido: true,
        secret: false,
        valor: "V",
      }),
    ).toBe("k = V");
  });

  it("resolvida nao secret com valor undefined usa string vazia", () => {
    // Mata mutante que troque `?? ""` por algo que imprima "undefined".
    expect(
      dicaDoToken({
        tipo: "var",
        bruto: "{{k}}",
        nome: "k",
        resolvido: true,
        secret: false,
        valor: undefined,
      }),
    ).toBe("k = ");
  });

  it("resolvida nao secret com valor vazio mostra so 'nome = '", () => {
    expect(
      dicaDoToken({
        tipo: "var",
        bruto: "{{k}}",
        nome: "k",
        resolvido: true,
        secret: false,
        valor: "",
      }),
    ).toBe("k = ");
  });

  it("secret tem prioridade sobre exibir valor mesmo se valor presente", () => {
    // Defesa: se por engano valor vier preenchido num secret, a dica nao vaza.
    expect(
      dicaDoToken({
        tipo: "var",
        bruto: "{{k}}",
        nome: "k",
        resolvido: true,
        secret: true,
        valor: "nao-deve-aparecer",
      }),
    ).toBe("k: (secreto)");
  });
});
