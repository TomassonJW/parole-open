#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod exports;
mod jobs;
mod models;
#[cfg(target_os = "macos")]
mod native_check;
mod presentation;
mod presentation_store;
mod topics;

fn main() {
    #[cfg(target_os = "macos")]
    if let Some(code) = native_check::requested() {
        std::process::exit(code);
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(jobs::RunState::default())
        .manage(audio::AudioState::default())
        .manage(topics::TopicState::default())
        .setup(|app| {
            use tauri::Manager;
            let root = app
                .path()
                .app_local_data_dir()
                .map_err(std::io::Error::other)?;
            std::fs::create_dir_all(&root)?;
            app.manage(presentation_store::Store::new(&root));
            jobs::recover_jobs(app.handle()).map_err(std::io::Error::other)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            models::model_status,
            models::prepare_models,
            models::install_report_model,
            jobs::list_jobs,
            jobs::start_job,
            jobs::resume_job,
            jobs::cancel_job,
            audio::load_audio_at,
            topics::load_topic_candidates,
            topics::prepare_topic_candidates,
            exports::save_speaker_names,
            exports::export_job,
            presentation::load_presentation,
            presentation::save_presentation,
            presentation::preview_presentation,
            presentation::load_presentation_defaults,
            presentation::save_presentation_defaults,
        ])
        .run(tauri::generate_context!())
        .expect("Impossible de démarrer Parole");
}
