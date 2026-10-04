use crate::{
    jobs::{folder, load, JobView, RunState},
    presentation_store::Store,
};
use parole_core::transcript_presentation::{
    render_presented_markdown, render_presented_txt, validate_preferences_for_job,
    PresentationPreferences,
};
use parole_core::{
    language::load_report_state, render_srt, render_vtt, save_job,
    verified_report::render_synthesis_with_sources,
};
use std::{collections::BTreeMap, fs, io::Write, path::Path};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub fn save_speaker_names(
    app: AppHandle,
    state: State<'_, RunState>,
    id: String,
    names: BTreeMap<String, String>,
) -> Result<JobView, String> {
    state.with_idle(|| {
        let mut job = load(&app, &id)?;
        for (speaker, name) in names {
            if name.trim().is_empty() || name.len() > 100 {
                return Err("Nom du locuteur invalide".into());
            }
            if job
                .segments
                .iter()
                .any(|s| s.speaker_id.as_deref() == Some(&speaker))
            {
                job.speaker_names.insert(speaker, name.trim().into());
            }
        }
        if job.report.is_some() {
            let state = load_report_state(&folder(&app, &id)?.join("compte-rendu-etat.json"))?
                .ok_or("État du compte rendu introuvable : modification non enregistrée")?;
            let report = state
                .report
                .ok_or("Compte rendu incomplet : modification non enregistrée")?;
            job.report = Some(render_synthesis_with_sources(&job, &report));
            job.report_format_version = 1;
        }
        save_job(&job, &folder(&app, &id)?.join("travail.json"))
            .map_err(|_| "Modification non enregistrée")?;
        Ok(JobView { id, job })
    })
}

fn checked_destination(path: &Path, data_root: &Path, format: &str) -> Result<(), String> {
    if !path.is_absolute()
        || !path
            .extension()
            .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case(format))
    {
        return Err("Choisissez un emplacement avec la bonne extension".into());
    }
    let parent = fs::canonicalize(path.parent().ok_or("Dossier d'export introuvable")?)
        .map_err(|_| "Dossier d'export introuvable")?;
    let data_root =
        fs::canonicalize(data_root).map_err(|_| "Données de l'application introuvables")?;
    if parent.starts_with(&data_root) {
        return Err("L'export ne peut pas être placé dans les données de l'application".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn export_job(
    app: AppHandle,
    id: String,
    format: String,
    default_name: String,
    presentation: Option<PresentationPreferences>,
) -> Result<Option<String>, String> {
    let store = app.state::<Store>();
    let job = store.read_job(&id)?;
    if !parole_core::export_format_allowed(&job, &format) {
        return Err("Ce format ne peut pas signaler qu'un travail est interrompu. Choisissez Texte, Markdown ou Word, ou reprenez le traitement.".into());
    }
    // Les formats d'archive/sous-titres ne dépendent jamais des préférences.
    // Les choix des formats lisibles sont figés avant d'ouvrir le dialogue natif.
    let selected = if matches!(format.as_str(), "txt" | "md" | "docx") {
        let preferences = match presentation {
            Some(value) => value,
            None => {
                let state = store.load_job(&id)?;
                if !state.writable {
                    return Err(
                        "Préférences de présentation endommagées : export lisible interrompu"
                            .into(),
                    );
                }
                state.preferences
            }
        };
        validate_preferences_for_job(&preferences, &job)?;
        Some(preferences)
    } else {
        None
    };
    let bytes: Vec<u8> = match format.as_str() {
        "txt" => render_presented_txt(&job, selected.as_ref().unwrap())?.into_bytes(),
        "md" => render_presented_markdown(&job, selected.as_ref().unwrap())?.into_bytes(),
        "docx" => parole_core::docx::render_presented_docx(&job, selected.as_ref().unwrap())?,
        "json" => parole_core::render_export_json(&job).map_err(|_| "Export JSON impossible")?,
        "srt" => render_srt(&job).into_bytes(),
        "vtt" => render_vtt(&job).into_bytes(),
        _ => return Err("Format d'export inconnu".into()),
    };
    let safe_name: String = default_name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .take(100)
        .collect();
    let safe_name = if safe_name.trim().is_empty() {
        "transcription"
    } else {
        safe_name.trim()
    };
    let suggested = format!("{safe_name}.{format}");
    let app_for_dialog = app.clone();
    let extension = format.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app_for_dialog
            .dialog()
            .file()
            .set_title("Enregistrer la transcription")
            .set_file_name(suggested)
            .add_filter(extension.to_uppercase(), &[extension.as_str()])
            .blocking_save_file()
    })
    .await
    .map_err(|_| "Fenêtre d'enregistrement indisponible")?;
    let Some(destination) = picked else {
        return Ok(None);
    };
    let destination = destination
        .into_path()
        .map_err(|_| "Emplacement non pris en charge")?;
    let root = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "Données de l'application introuvables")?;
    checked_destination(&destination, &root, &format)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|_| {
            "Fichier déjà présent ou emplacement non accessible : choisissez un autre nom"
        })?;
    file.write_all(&bytes)
        .map_err(|_| "Impossible d'écrire le fichier choisi")?;
    Ok(Some(destination.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn export_cannot_target_internal_data_or_wrong_extension() {
        let root = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        assert!(checked_destination(&root.join("travail.json"), &root, "json").is_err());
        assert!(checked_destination(&root.join("externe.txt"), &root, "json").is_err());
        assert!(
            checked_destination(&root.parent().unwrap().join("ailleurs.json"), &root, "json")
                .is_ok()
        );
    }
}
