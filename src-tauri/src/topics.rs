//! Deux commandes locales bornées : lecture seule ou préparation explicite.
use crate::{audio, jobs};
use parole_core::topic_access::{TopicLibrary, TopicSnapshot};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{State, Webview};

const UNAVAILABLE: &str = "Propositions indisponibles ou non vérifiables";
const BUSY: &str = "Une consultation des propositions est déjà en cours";

#[derive(Default)]
pub struct TopicState(Arc<AtomicBool>);
struct Release(Arc<AtomicBool>);
impl Drop for Release {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
impl TopicState {
    fn acquire(&self) -> Result<Release, String> {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| BUSY.to_string())?;
        Ok(Release(self.0.clone()))
    }
}
async fn snapshot(
    app: tauri::AppHandle,
    webview: Webview,
    state: State<'_, TopicState>,
    id: String,
    prepare: bool,
) -> Result<TopicSnapshot, String> {
    if !audio::local_main(&webview) {
        return Err(UNAVAILABLE.into());
    }
    let guard = state.acquire()?;
    let root = jobs::jobs_dir(&app).map_err(|_| UNAVAILABLE.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        let library = TopicLibrary::open(&root)?;
        if prepare {
            library.prepare(&id)
        } else {
            library.load(&id)
        }
    })
    .await
    .map_err(|_| UNAVAILABLE.to_string())?
}
#[tauri::command]
pub async fn load_topic_candidates(
    app: tauri::AppHandle,
    webview: Webview,
    state: State<'_, TopicState>,
    id: String,
) -> Result<TopicSnapshot, String> {
    snapshot(app, webview, state, id, false).await
}
#[tauri::command]
pub async fn prepare_topic_candidates(
    app: tauri::AppHandle,
    webview: Webview,
    state: State<'_, TopicState>,
    id: String,
) -> Result<TopicSnapshot, String> {
    snapshot(app, webview, state, id, true).await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exclusif_et_libere_apres_erreur() {
        let state = TopicState::default();
        let guard = state.acquire().unwrap();
        assert_eq!(state.acquire().err().as_deref(), Some(BUSY));
        drop(guard);
        assert!(state.acquire().is_ok());
    }
}
