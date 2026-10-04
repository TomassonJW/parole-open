use parole_core::{diarization::DiarizationConfig, language::MODELE_TEXTE_RECOMMANDE};

#[cfg(test)]
mod report_catalog_tests {
    use super::*;

    #[test]
    fn pinned_catalog_and_unknown_id_are_fail_closed() {
        let baseline = report_model("baseline").unwrap();
        assert_eq!(baseline.sha256, MODELE_TEXTE_RECOMMANDE.sha256);
        let candidate = report_model("qwen3-4b-instruct-2507-q4_k_m").unwrap();
        assert_eq!(candidate.bytes, 2_497_280_736);
        assert_eq!(
            candidate.sha256,
            "2fde00ce69dd4899c70d020845e2638353015bba0fdf161b3eb965f2bca4464e"
        );
        assert!(candidate
            .url
            .contains("/ae44f08e1392f39c0e474af10c3ff8355c8b6688/"));
        assert!(report_model("unknown").is_err());
        assert!(report_model("../baseline").is_err());
    }

    #[test]
    fn report_model_metadata_verifies_file_size_and_digest() {
        let model = report_model("qwen3-4b-instruct-2507-q4_k_m").unwrap();
        let spec = model.spec();
        assert_eq!(spec.size_bytes, model.bytes);
        assert_eq!(spec.sha256, model.sha256);
    }

    #[test]
    fn absent_or_corrupt_optional_model_is_not_installed_without_network() {
        let dir = std::env::temp_dir().join(format!("parole-model-test-{}", std::process::id()));
        let model = report_model("qwen3-4b-instruct-2507-q4_k_m").unwrap();
        let path = dir.join(&model.file);
        assert!(!report_installed(&dir, &model));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, b"invalid").unwrap();
        assert!(!report_installed(&dir, &model));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn modele_intermediaire_ne_requiert_pas_le_modele_de_traduction() {
        let models = report_models().unwrap();
        let status = ModelStatus {
            core_installed: true,
            installed: false,
            missing: vec!["Modèle de traduction absent".into()],
            report_models: models
                .into_iter()
                .map(|model| ReportModelStatus {
                    installed: model.id != "baseline",
                    model,
                })
                .collect(),
        };
        assert!(status.ready_for(false));
        assert!(!status.ready_for(true));
        let blocked = ModelStatus {
            core_installed: false,
            ..status
        };
        assert!(!blocked.ready_for(false));
    }
}

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};
use tauri::{AppHandle, Emitter, Manager};

const URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo-q5_0.bin";
const SHA256: &str = "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2";
const SIZE: u64 = 574_041_195;

#[cfg(test)]
use parole_core::report_models::report_models;
use parole_core::report_models::{new_report_models, report_model_for_generation};
pub use parole_core::report_models::{report_model, ReportModel};

fn report_installed(dir: &std::path::Path, model: &ReportModel) -> bool {
    hash_matches(&dir.join(&model.file), model.bytes, &model.sha256)
}

pub fn report_model_path(app: &AppHandle, id: &str) -> Result<(PathBuf, ReportModel), String> {
    let model = report_model_for_generation(id)?;
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "Dossier de données inaccessible")?
        .join("models");
    if !report_installed(&dir, &model) {
        return Err(format!("Modèle de compte rendu « {} » absent ou endommagé. Installez-le explicitement pour reprendre ce travail.", model.name));
    }
    Ok((dir.join(&model.file), model))
}

#[derive(Clone, Serialize)]
pub struct ReportModelStatus {
    #[serde(flatten)]
    pub model: ReportModel,
    pub installed: bool,
}

#[derive(Clone, Serialize)]
pub struct ModelStatus {
    /// Moteurs de transcription et de séparation des voix, sans modèle de langue.
    pub core_installed: bool,
    /// Compatibilité avec l'état historique (socle + modèle de base).
    pub installed: bool,
    pub missing: Vec<String>,
    pub report_models: Vec<ReportModelStatus>,
}

impl ModelStatus {
    pub fn ready_for(&self, needs_baseline: bool) -> bool {
        self.core_installed
            && (!needs_baseline
                || self
                    .report_models
                    .iter()
                    .any(|m| m.model.id == "baseline" && m.installed))
    }
}

pub fn model_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|_| "Dossier de données inaccessible")?
        .join("models/ggml-large-v3-turbo-q5_0.bin"))
}
pub fn text_model_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|_| "Dossier de données inaccessible")?
        .join("models")
        .join(MODELE_TEXTE_RECOMMANDE.file_name))
}
fn hash_matches(path: &PathBuf, size: u64, expected: &str) -> bool {
    let Ok(mut f) = File::open(path) else {
        return false;
    };
    let Ok(metadata) = f.metadata() else {
        return false;
    };
    if metadata.len() != size {
        return false;
    }
    let mut sha = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        match f.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => sha.update(&buffer[..n]),
            Err(_) => return false,
        }
    }
    format!("{:x}", sha.finalize()) == expected
}

pub fn native_path(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    if let Some(path) =
        std::env::var_os(format!("PAROLE_{}", name.to_uppercase().replace('-', "_")))
    {
        return Ok(PathBuf::from(path));
    }
    let mut binary = name.to_string();
    if cfg!(windows) {
        binary.push_str(".exe");
    }
    Ok(app
        .path()
        .resource_dir()
        .map_err(|_| "Ressources de l'application introuvables")?
        .join("native")
        .join(binary))
}

pub fn diarization_config(app: &AppHandle) -> Result<DiarizationConfig, String> {
    let native = app
        .path()
        .resource_dir()
        .map_err(|_| "Ressources de l'application introuvables")?
        .join("native");
    let library = if cfg!(windows) {
        "sherpa-onnx-c-api.dll"
    } else if cfg!(target_os = "macos") {
        "libsherpa-onnx-c-api.dylib"
    } else {
        "libsherpa-onnx-c-api.so"
    };
    let config = DiarizationConfig::new(
        native.join(library),
        native.join("segmentation.onnx"),
        native.join("embedding.onnx"),
    );
    config.validate().map_err(|e| e.to_string())?;
    if !hash_matches(
        &config.segmentation_model,
        5992913,
        "220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079",
    ) || !hash_matches(
        &config.embedding_model,
        26485263,
        "c59158379255ad66e161679cca6af8d52d51e389e3224ab7d7a7baae295c2db5",
    ) {
        return Err("Modèle de séparation des voix endommagé : réinstallez l'application".into());
    }
    Ok(config)
}

#[tauri::command]
pub fn model_status(app: AppHandle) -> Result<ModelStatus, String> {
    let mut missing = Vec::new();
    if !hash_matches(&model_path(&app)?, SIZE, SHA256) {
        missing.push("Modèle de transcription absent ou endommagé".to_string());
    }
    let text_installed = hash_matches(
        &text_model_path(&app)?,
        MODELE_TEXTE_RECOMMANDE.size_bytes,
        MODELE_TEXTE_RECOMMANDE.sha256,
    );
    for (name, label) in [
        ("whisper-cli", "Moteur de transcription"),
        ("llama-completion", "Moteur de traduction et compte rendu"),
        ("ffmpeg", "Décodeur audio/vidéo"),
        ("ffprobe", "Analyseur audio/vidéo"),
    ] {
        if !native_path(&app, name)?.is_file() {
            missing.push(format!("{label} absent"));
        }
    }
    if diarization_config(&app).is_err() {
        missing.push("Moteur ou modèles de séparation des voix absents".to_string());
    }
    let core_installed = missing.is_empty();
    if !text_installed {
        missing.push("Modèle de traduction et compte rendu absent ou endommagé".to_string());
    }
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "Dossier de données inaccessible")?
        .join("models");
    let report_models = new_report_models()?
        .into_iter()
        .map(|model| {
            let installed = if model.id == "baseline" {
                text_installed
            } else {
                report_installed(&dir, &model)
            };
            ReportModelStatus { model, installed }
        })
        .collect();
    Ok(ModelStatus {
        core_installed,
        installed: missing.is_empty(),
        missing,
        report_models,
    })
}

#[tauri::command]
pub async fn install_report_model(app: AppHandle, id: String) -> Result<ModelStatus, String> {
    let model = report_model_for_generation(&id)?;
    if id == "baseline" {
        return Err("Le modèle de base se prépare avec « Préparer les modèles ».".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let dir = app
            .path()
            .app_local_data_dir()
            .map_err(|_| "Dossier de données inaccessible")?
            .join("models");
        download_one(
            &app,
            dir.join(&model.file),
            &model.url,
            model.bytes,
            &model.sha256,
        )?;
        model_status(app)
    })
    .await
    .map_err(|_| "Installation du modèle de compte rendu interrompue".to_string())?
}

#[tauri::command]
pub async fn prepare_models(app: AppHandle) -> Result<ModelStatus, String> {
    tauri::async_runtime::spawn_blocking(move || download(&app))
        .await
        .map_err(|_| "Préparation interrompue".to_string())?
}
fn download(app: &AppHandle) -> Result<ModelStatus, String> {
    for name in ["ffmpeg", "ffprobe", "whisper-cli", "llama-completion"] {
        if !native_path(app, name)?.is_file() {
            return Err(
                "Installation incomplète : réinstallez l'application avant de préparer les modèles"
                    .into(),
            );
        }
    }
    diarization_config(app)?;
    download_one(app, model_path(app)?, URL, SIZE, SHA256)?;
    download_one(
        app,
        text_model_path(app)?,
        MODELE_TEXTE_RECOMMANDE.url,
        MODELE_TEXTE_RECOMMANDE.size_bytes,
        MODELE_TEXTE_RECOMMANDE.sha256,
    )?;
    model_status(app.clone())
}
fn download_one(
    app: &AppHandle,
    path: PathBuf,
    url: &str,
    size: u64,
    sha256: &str,
) -> Result<(), String> {
    if hash_matches(&path, size, sha256) {
        return Ok(());
    }
    fs::create_dir_all(path.parent().ok_or("Dossier introuvable")?)
        .map_err(|_| "Impossible de créer le dossier des modèles")?;
    let free = fs2::available_space(path.parent().ok_or("Dossier introuvable")?)
        .map_err(|_| "Espace disque inconnu")?;
    if free < size * 2 {
        return Err("Espace disque insuffisant pour installer le modèle".into());
    }
    let part = path.with_extension("bin.part");
    let saved = part.metadata().map(|m| m.len()).unwrap_or(0);
    let request = if saved > 0 && saved < size {
        ureq::get(url).set("Range", &format!("bytes={saved}-"))
    } else {
        ureq::get(url)
    };
    let response = request
        .call()
        .map_err(|_| "Téléchargement du modèle impossible. Vérifiez la connexion.")?;
    let resume = saved > 0 && response.status() == 206;
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(resume)
        .truncate(!resume)
        .open(&part)
        .map_err(|_| "Écriture du modèle impossible")?;
    let mut reader = response.into_reader();
    let mut received = if resume { saved } else { 0 };
    let mut last_reported = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|_| "Téléchargement interrompu. Il reprendra au prochain essai.")?;
        if n == 0 {
            break;
        }
        if received.saturating_add(n as u64) > size {
            return Err("Téléchargement plus volumineux que prévu : fichier refusé".into());
        }
        file.write_all(&buffer[..n])
            .map_err(|_| "Espace disque insuffisant")?;
        received += n as u64;
        if received.saturating_sub(last_reported) >= size / 100 || received == size {
            let _ = app.emit(
                "model-progress",
                serde_json::json!({"received":received,"total":size,"model":path.file_name().and_then(|n|n.to_str()).unwrap_or("modèle")}),
            );
            last_reported = received;
        }
    }
    file.sync_all()
        .map_err(|_| "Enregistrement du modèle impossible")?;
    if !hash_matches(&part, size, sha256) {
        return Err("Modèle incomplet ou empreinte incorrecte ; réessayez.".into());
    }
    fs::rename(part, path).map_err(|_| "Installation du modèle impossible")?;
    Ok(())
}
