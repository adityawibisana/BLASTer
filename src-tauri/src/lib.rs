pub mod abif;
pub mod analysis;
pub mod app;
pub mod assembly;
pub mod blast;
pub mod compile;
pub mod export;
pub mod fasta;
pub mod importer;
pub mod local_blast;
pub mod model;
pub mod pairs;
pub mod pdf;
pub mod primers;
pub mod results_file;
pub mod seq;
pub mod store;
pub mod trim;
pub mod views;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let documents_dir = app.path().document_dir().or_else(|_| app.path().home_dir())?;
            app.manage(app::AppState {
                project: Arc::new(Mutex::new(None)),
                running: AtomicBool::new(false),
                cancel: AtomicBool::new(false),
                save_signal: tokio::sync::Notify::new(),
                config_dir,
                documents_dir,
            });
            tauri::async_runtime::spawn(app::saver_loop(app.handle().clone()));
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<app::AppState>();
                // Flush unsaved changes before the window closes.
                let _ = app::save_now(&state);
                // A search is running: ask first. The window shows the question and
                // calls `quit_app` if the user still wants to close.
                let open = app::open_groups(&state);
                if open > 0 {
                    api.prevent_close();
                    let _ = window.emit("confirm-close", open);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app::get_defaults,
            app::save_defaults,
            app::recent_projects,
            app::forget_recent,
            app::primer_library,
            app::get_project,
            app::create_project,
            app::open_project,
            app::close_project,
            app::save_project_as,
            app::get_trace,
            app::preview_trim,
            app::set_trim,
            app::reset_read,
            app::set_direction,
            app::set_inclusion,
            app::update_settings,
            app::get_sample_result,
            app::get_hit_detail,
            app::compile_results,
            app::export_excel,
            app::export_fasta,
            app::export_pdf,
            app::confirm_pair,
            app::confirm_all_pairs,
            app::reject_pair,
            app::unpair,
            app::swap_pair,
            app::set_pair_use_only,
            app::export_raw,
            app::start_blast,
            app::cancel_blast,
            app::resubmit_group,
            app::import_blast_results,
            app::open_on_website,
            app::check_local_blast,
            app::quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the application");
}
