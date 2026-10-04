use crate::presentation_store::Store;
use parole_core::transcript_presentation::{
    project, validate_view, PresentationPreferences, PresentationSnapshot, PresentationState,
    ViewOptions,
};
use tauri::State;

#[tauri::command]
pub fn load_presentation(store: State<'_, Store>, id: String) -> Result<PresentationState, String> {
    store.read_job(&id)?;
    store.load_job(&id)
}

#[tauri::command]
pub fn save_presentation(
    store: State<'_, Store>,
    id: String,
    preferences: PresentationPreferences,
    expected_revision: u64,
) -> Result<PresentationState, String> {
    let job = store.read_job(&id)?;
    store.save_job(&id, &job, preferences, expected_revision)
}

#[tauri::command]
pub fn preview_presentation(
    store: State<'_, Store>,
    id: String,
    options: ViewOptions,
) -> Result<PresentationSnapshot, String> {
    validate_view(&options)?;
    let job = store.read_job(&id)?;
    project(&id, &job, &options)
}

#[tauri::command]
pub fn load_presentation_defaults(store: State<'_, Store>) -> Result<PresentationState, String> {
    store.load_defaults()
}

#[tauri::command]
pub fn save_presentation_defaults(
    store: State<'_, Store>,
    preferences: PresentationPreferences,
    expected_revision: u64,
) -> Result<PresentationState, String> {
    store.save_defaults(preferences, expected_revision)
}
