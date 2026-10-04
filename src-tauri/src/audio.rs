use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{ipc::Response, State, Webview};

const UNAVAILABLE: &str = "Extrait audio indisponible ou non vérifiable";
const BUSY: &str = "Une acquisition audio est déjà en cours";

#[derive(Default)]
pub struct AudioState(Arc<AtomicBool>);
struct Release(Arc<AtomicBool>);
impl Drop for Release {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
impl AudioState {
    fn acquire(&self) -> Result<Release, String> {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| BUSY.to_string())?;
        Ok(Release(self.0.clone()))
    }
}

pub(crate) fn local_main(webview: &Webview) -> bool {
    if webview.label() != "main" {
        return false;
    }
    let Ok(url) = webview.url() else {
        return false;
    };
    let origin = url.origin().ascii_serialization();
    let production =
        (url.scheme() == "tauri" && url.host_str() == Some("localhost") && url.port().is_none())
            || origin == "http://tauri.localhost";
    let development = cfg!(debug_assertions)
        && matches!(
            origin.as_str(),
            "http://localhost:1420" | "http://127.0.0.1:1420"
        );
    (production || development) && matches!(url.path(), "/" | "/index.html")
}

#[tauri::command]
pub async fn load_audio_at(
    app: tauri::AppHandle,
    webview: Webview,
    state: State<'_, AudioState>,
    id: String,
    at_ms: u64,
) -> Result<Response, String> {
    if !local_main(&webview) {
        return Err(UNAVAILABLE.into());
    }
    let guard = state.acquire()?;
    let root = jobs::jobs_dir(&app).map_err(|_| UNAVAILABLE.to_string())?;
    let output = tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        let library = parole_core::audio_access::AudioLibrary::open(&root)?;
        parole_core::audio_playback::encode(&library, &id, at_ms)
    })
    .await
    .map_err(|_| UNAVAILABLE.to_string())?;
    output.map(Response::new)
}

use crate::jobs;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_simultaneous_acquisitions_and_releases_on_failure() {
        let state = AudioState::default();
        let first = state.acquire().unwrap();
        assert_eq!(state.acquire().err().as_deref(), Some(BUSY));
        drop(first);
        assert!(state.acquire().is_ok());
    }
}
