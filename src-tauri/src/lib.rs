pub mod abif;
pub mod analysis;
pub mod app;
pub mod assembly;
pub mod blast;
pub mod compile;
pub mod export;
pub mod fasta;
pub mod importer;
pub mod model;
pub mod pairs;
pub mod pdf;
pub mod primers;
pub mod seq;
pub mod store;
pub mod trim;
pub mod views;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tauri::Manager;

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
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                // Flush unsaved changes before the window closes.
                let _ = app::save_now(&window.state::<app::AppState>());
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
            app::reject_pair,
            app::unpair,
            app::swap_pair,
            app::set_pair_use_only,
            app::export_raw,
            app::start_blast,
            app::cancel_blast,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the application");
}
