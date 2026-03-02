#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod chroma;
mod chunker;
mod clustering;
mod config;
mod fragment;
mod grpc_client;
mod markdown;
mod pipeline;
mod search;
mod sidecar;
mod sources;
mod state;
mod threads;
mod watcher;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("scrapbin=debug,info")
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|_app| {
            // Initialize app data directory structure
            if let Err(e) = config::init_app_dirs() {
                tracing::error!("Failed to initialize app data directory: {}", e);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Config
            config::config_get,
            config::config_set,
            config::config_get_data_dir,
            // Watcher
            watcher::watcher_start,
            watcher::watcher_stop,
            watcher::watcher_is_active,
            watcher::watcher_get_vault_path,
            // Sidecar management
            sidecar::sidecar_start_all,
            sidecar::sidecar_stop_all,
            sidecar::sidecar_status,
            // Chroma
            chroma::client::chroma_health_check,
            chroma::collections::chroma_list_collections,
            chroma::collections::chroma_get_collection_stats,
            // Clustering
            clustering::clustering_run,
            clustering::clustering_get_all,
            clustering::clustering_get_fragments,
            clustering::clustering_get_orphans,
            clustering::clustering_merge,
            clustering::clustering_split,
            clustering::clustering_move_fragment,
            clustering::clustering_rename,
            clustering::clustering_pin_label,
            clustering::clustering_get_positions,
            // Threads
            threads::threads_detect,
            threads::threads_get_all,
            threads::threads_name,
            threads::threads_confirm,
            threads::threads_dismiss,
            // Sources
            sources::twitter::source_twitter_import,
            sources::readwise::source_readwise_import,
            sources::readwise::source_readwise_configure,
            sources::readwise::source_readwise_check_connection,
            sources::podcasts::source_podcasts_import,
            // Pipeline
            pipeline::pipeline_index_vault,
            pipeline::pipeline_index_file,
            pipeline::pipeline_get_stats,
            pipeline::pipeline_create_note,
            pipeline::pipeline_get_recent,
            // Search
            search::search_all,
            search::search_collection,
            // Fragment querying
            search::list_fragments,
            search::get_fragment,
            search::get_disposition_counts,
            search::get_inbox,
            // Fragment mutation
            pipeline::set_disposition,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                // Graceful shutdown: stop watchers and sidecars
                watcher::stop_watching();
                let _ = sidecar::stop_all();
            }
        });
}
