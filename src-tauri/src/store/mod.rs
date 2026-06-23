// Modulo `store` — modelo de dados file-based e camada de persistencia.
//
// Organizacao:
//   models   — schema serializavel (espelhado em src/lib/types.ts), POJOs puros.
//   error    — StoreError (serializavel pro IPC).
//   slug     — sanitizacao de nomes de arquivo (LOGICA PURA, mutation testing).
//   parser   — parse/stringify YAML <-> structs (LOGICA PURA, mutation testing).
//   fs_store — I/O: le/grava a arvore da colecao no disco.
//   watcher  — watcher de filesystem que emite `collection-changed`.
//   commands — comandos IPC (validam input nao-confiavel).

pub mod collection_ops;
pub mod commands;
pub mod env_ops;
pub mod error;
pub mod fs_store;
pub mod models;
pub mod parser;
pub mod slug;
pub mod tree_ops;
pub mod watcher;
