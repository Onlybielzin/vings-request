// Modulo `http` — engine de envio de requests (F4).
//
// Organizacao:
//   types    — RequestData/ResponseData/HttpError serializaveis (espelho em src/lib/http-types.ts).
//   engine   — montagem + disparo via reqwest (LOGICA com I/O async, sem panic).
//   commands — comando IPC `send_request` (registrar no lib.rs na Integracao).
//
// Convencoes de serde: camelCase no IPC, igual ao resto do projeto.
// O modelo de dados em disco (store::models::RequestItem) NAO e usado direto no
// envio: a F4 trabalha com um RequestData mais enxuto, montado pelo front a
// partir do RequestItem. Isso desacopla a engine do schema de persistencia.

pub mod commands;
pub mod engine;
pub mod oauth;
pub mod types;
