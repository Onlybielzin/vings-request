// ruan — cliente HTTP file-based. Entry point do backend Tauri.
// Os comandos IPC reais sao adicionados por feature (M1+): store (F1), http, etc.

pub mod app_state;
pub mod http;
pub mod store;

use store::watcher::CollectionWatchers;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // Estado: watchers de filesystem por colecao (F1).
        .manage(CollectionWatchers::new())
        .invoke_handler(tauri::generate_handler![
            // F1 — store basico
            store::commands::open_collection,
            store::commands::save_request,
            store::commands::create_folder,
            store::commands::delete_request,
            store::commands::watch_collection,
            store::commands::unwatch_collection,
            // F2 — colecoes (criar) + persistencia da lista de abertas
            app_state::create_collection,
            app_state::load_open_collections_cmd,
            app_state::save_open_collections_cmd,
            // F3 — arvore (CRUD de request/pasta + mover/duplicar/renomear)
            store::tree_ops::create_request_cmd,
            store::tree_ops::rename_item,
            store::tree_ops::duplicate_item,
            store::tree_ops::move_item,
            // F4 — envio HTTP
            http::commands::send_request,
            // F9 — environments & variaveis (por colecao + globais)
            store::env_ops::list_environments,
            store::env_ops::save_environment_cmd,
            store::env_ops::delete_environment_cmd,
            app_state::globals::load_global_vars_cmd,
            app_state::globals::save_global_vars_cmd,
            // F11 — OAuth2 (obtencao de token; demais auth e aplicada no front)
            http::oauth::oauth2_token,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
