import { describe, it, expect } from "vitest";
import {
  type Variable,
  type Environment,
  type EstadoColecaoEnv,
  acharEnvAtivo,
  construirScopes,
  buscarVar,
  resolverVar,
} from "./envScopes";

function v(
  name: string,
  value: string,
  enabled = true,
  secret = false,
): Variable {
  return { name, value, enabled, secret };
}

function env(name: string, variables: Variable[]): Environment {
  return { name, variables };
}

function estado(p: Partial<EstadoColecaoEnv> = {}): EstadoColecaoEnv {
  return {
    environments: [],
    activeEnvName: null,
    collectionVars: [],
    globalVars: [],
    runtimeVars: {},
    ...p,
  };
}

describe("acharEnvAtivo", () => {
  it("retorna undefined quando activeEnvName e null", () => {
    expect(acharEnvAtivo([env("a", [])], null)).toBeUndefined();
  });

  it("retorna undefined quando o nome nao bate nenhum", () => {
    expect(acharEnvAtivo([env("a", [])], "b")).toBeUndefined();
  });

  it("acha o environment pelo nome", () => {
    const e = env("prod", [v("x", "1")]);
    expect(acharEnvAtivo([env("dev", []), e], "prod")).toBe(e);
  });

  it("retorna a PRIMEIRA ocorrencia quando ha nomes duplicados", () => {
    const e1 = env("dup", [v("x", "1")]);
    const e2 = env("dup", [v("x", "2")]);
    expect(acharEnvAtivo([e1, e2], "dup")).toBe(e1);
  });

  it("retorna undefined para lista vazia mesmo com nome dado", () => {
    expect(acharEnvAtivo([], "qualquer")).toBeUndefined();
  });

  it("trata string vazia como nome buscavel (nao como ausencia)", () => {
    const e = env("", [v("x", "1")]);
    // activeEnvName "" nao e null -> deve procurar e achar o env de nome "".
    expect(acharEnvAtivo([env("dev", []), e], "")).toBe(e);
  });
});

describe("construirScopes", () => {
  it("env vazio quando nao ha ativo", () => {
    const s = construirScopes(
      estado({ environments: [env("prod", [v("a", "1")])] }),
    );
    expect(s.env).toEqual([]);
  });

  it("usa as variaveis do environment ativo", () => {
    const s = construirScopes(
      estado({
        environments: [env("prod", [v("a", "1")])],
        activeEnvName: "prod",
      }),
    );
    expect(s.env).toEqual([v("a", "1")]);
  });

  it("popula collection, global e runtime", () => {
    const s = construirScopes(
      estado({
        collectionVars: [v("c", "cv")],
        globalVars: [v("g", "gv")],
        runtimeVars: { r: "rv" },
      }),
    );
    expect(s.collection).toEqual([v("c", "cv")]);
    expect(s.global).toEqual([v("g", "gv")]);
    expect(s.runtime).toEqual({ r: "rv" });
  });

  it("copia os arrays (nao compartilha referencia)", () => {
    const colVars = [v("c", "cv")];
    const s = construirScopes(estado({ collectionVars: colVars }));
    expect(s.collection).not.toBe(colVars);
    expect(s.collection).toEqual(colVars);
  });

  it("copia o runtime (nao compartilha referencia)", () => {
    const rt = { r: "rv" };
    const s = construirScopes(estado({ runtimeVars: rt }));
    expect(s.runtime).not.toBe(rt);
    expect(s.runtime).toEqual(rt);
  });

  it("env fica vazio quando activeEnvName aponta para nome inexistente", () => {
    const s = construirScopes(
      estado({
        environments: [env("prod", [v("a", "1")])],
        activeEnvName: "nao-existe",
      }),
    );
    expect(s.env).toEqual([]);
  });

  it("NAO filtra desabilitadas em construirScopes (mantem arrays intactos)", () => {
    // construirScopes preserva enabled=false; o filtro e do lookup, nao da montagem.
    const s = construirScopes(
      estado({
        environments: [env("prod", [v("a", "1", false), v("b", "2", true)])],
        activeEnvName: "prod",
        collectionVars: [v("c", "cv", false)],
        globalVars: [v("g", "gv", false)],
      }),
    );
    expect(s.env).toEqual([v("a", "1", false), v("b", "2", true)]);
    expect(s.collection).toEqual([v("c", "cv", false)]);
    expect(s.global).toEqual([v("g", "gv", false)]);
  });

  it("copia o array do environment ativo (nao compartilha referencia)", () => {
    const envVars = [v("a", "1")];
    const s = construirScopes(
      estado({
        environments: [env("prod", envVars)],
        activeEnvName: "prod",
      }),
    );
    expect(s.env).not.toBe(envVars);
    expect(s.env).toEqual(envVars);
  });

  it("copia o array global (nao compartilha referencia)", () => {
    const globVars = [v("g", "gv")];
    const s = construirScopes(estado({ globalVars: globVars }));
    expect(s.global).not.toBe(globVars);
    expect(s.global).toEqual(globVars);
  });
});

describe("buscarVar", () => {
  it("acha a primeira ocorrencia habilitada", () => {
    expect(buscarVar([v("a", "1"), v("a", "2")], "a")).toBe("1");
  });

  it("ignora desabilitadas e pega a proxima habilitada", () => {
    expect(buscarVar([v("a", "off", false), v("a", "on")], "a")).toBe("on");
  });

  it("retorna undefined se so houver desabilitada", () => {
    expect(buscarVar([v("a", "off", false)], "a")).toBeUndefined();
  });

  it("retorna undefined se nao houver o nome", () => {
    expect(buscarVar([v("b", "1")], "a")).toBeUndefined();
  });

  it("retorna string vazia se a variavel habilitada tiver valor vazio", () => {
    expect(buscarVar([v("a", "")], "a")).toBe("");
  });

  it("nao casa por nome se a variavel estiver desabilitada (mata && -> primeiro termo)", () => {
    // Se o && virasse so v.name===nome, este retornaria "x" em vez de undefined.
    expect(buscarVar([v("a", "x", false)], "a")).toBeUndefined();
  });

  it("nao casa por enabled se o nome for diferente (mata && -> so v.enabled)", () => {
    // Se o && virasse so v.enabled, este retornaria "x" em vez de undefined.
    expect(buscarVar([v("b", "x", true)], "a")).toBeUndefined();
  });

  it("retorna undefined para array vazio", () => {
    expect(buscarVar([], "a")).toBeUndefined();
  });

  it("respeita a ordem: habilitada anterior vence outra habilitada posterior", () => {
    expect(buscarVar([v("a", "primeira"), v("a", "segunda")], "a")).toBe(
      "primeira",
    );
  });
});

describe("resolverVar (precedencia runtime > env > collection > global)", () => {
  const base = (): EstadoColecaoEnv =>
    estado({
      environments: [env("prod", [v("k", "env")])],
      activeEnvName: "prod",
      collectionVars: [v("k", "col")],
      globalVars: [v("k", "glob")],
      runtimeVars: { k: "rt" },
    });

  it("runtime vence todos", () => {
    expect(resolverVar(construirScopes(base()), "k")).toBe("rt");
  });

  it("env vence collection e global", () => {
    const e = base();
    e.runtimeVars = {};
    expect(resolverVar(construirScopes(e), "k")).toBe("env");
  });

  it("collection vence global", () => {
    const e = base();
    e.runtimeVars = {};
    e.activeEnvName = null;
    expect(resolverVar(construirScopes(e), "k")).toBe("col");
  });

  it("global e o fallback final", () => {
    const e = estado({ globalVars: [v("k", "glob")] });
    expect(resolverVar(construirScopes(e), "k")).toBe("glob");
  });

  it("runtime com string vazia ainda vence (chave presente)", () => {
    const e = estado({
      collectionVars: [v("k", "col")],
      runtimeVars: { k: "" },
    });
    expect(resolverVar(construirScopes(e), "k")).toBe("");
  });

  it("desabilitada no env cai para collection", () => {
    const e = estado({
      environments: [env("prod", [v("k", "env", false)])],
      activeEnvName: "prod",
      collectionVars: [v("k", "col")],
    });
    expect(resolverVar(construirScopes(e), "k")).toBe("col");
  });

  it("retorna undefined quando nao existe em nenhum escopo", () => {
    expect(resolverVar(construirScopes(estado()), "inexistente")).toBeUndefined();
  });

  it("env vence collection (sem runtime nem global)", () => {
    const e = estado({
      environments: [env("prod", [v("k", "env")])],
      activeEnvName: "prod",
      collectionVars: [v("k", "col")],
    });
    expect(resolverVar(construirScopes(e), "k")).toBe("env");
  });

  it("desabilitada na collection cai para global", () => {
    const e = estado({
      collectionVars: [v("k", "col", false)],
      globalVars: [v("k", "glob")],
    });
    expect(resolverVar(construirScopes(e), "k")).toBe("glob");
  });

  it("runtime ausente (hasOwnProperty false) cai para env, nao retorna undefined", () => {
    const scopes = construirScopes(
      estado({
        environments: [env("prod", [v("k", "env")])],
        activeEnvName: "prod",
      }),
    );
    // runtime nao tem 'k' -> NAO deve retornar scopes.runtime['k'] (undefined),
    // deve continuar a cadeia e achar 'env'.
    expect(resolverVar(scopes, "k")).toBe("env");
  });

  it("runtime com chave de valor vazio vence env nao-vazio (chave presente)", () => {
    const e = estado({
      environments: [env("prod", [v("k", "env")])],
      activeEnvName: "prod",
      runtimeVars: { k: "" },
    });
    expect(resolverVar(construirScopes(e), "k")).toBe("");
  });

  it("nao confunde chaves herdadas do prototype no runtime", () => {
    // hasOwnProperty (e nao 'in') protege contra 'toString', 'constructor' etc.
    const scopes = construirScopes(estado());
    expect(resolverVar(scopes, "toString")).toBeUndefined();
    expect(resolverVar(scopes, "constructor")).toBeUndefined();
  });

  it("resolve usando apenas o global quando demais escopos vazios", () => {
    const e = estado({ globalVars: [v("only", "g")] });
    expect(resolverVar(construirScopes(e), "only")).toBe("g");
  });

  it("resolve usando apenas a collection quando demais escopos vazios", () => {
    const e = estado({ collectionVars: [v("only", "c")] });
    expect(resolverVar(construirScopes(e), "only")).toBe("c");
  });
});
