//! Catalogue local des modèles de compte rendu, épinglé au manifeste livré.
//! Ni la lecture du catalogue ni la sélection ne contactent le réseau.
use crate::language::{ModelSpec, MODELE_TEXTE_RECOMMANDE};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReportModel {
    pub id: String,
    pub name: String,
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
    pub url: String,
    /// Une absence de permission ferme les nouveaux usages, sans retirer les métadonnées.
    #[serde(default)]
    pub available_for_new_jobs: bool,
}

impl ReportModel {
    pub fn spec(&self) -> ModelSpec<'_> {
        ModelSpec {
            name: &self.name,
            file_name: &self.file,
            url: &self.url,
            sha256: &self.sha256,
            size_bytes: self.bytes,
            license: "Apache-2.0",
        }
    }
}

#[derive(Deserialize)]
struct Manifest {
    active_models: Vec<serde_json::Value>,
}

/// Refuse les branches mobiles et les chemins qui ne correspondent pas au fichier du catalogue.
pub fn pinned_model_url(model: &ReportModel) -> bool {
    if !model.url.starts_with("https://huggingface.co/") {
        return false;
    }
    let Some((_, resolved)) = model.url.split_once("/resolve/") else {
        return false;
    };
    let Some((revision, filename)) = resolved.split_once('/') else {
        return false;
    };
    revision.len() == 40
        && revision.bytes().all(|b| b.is_ascii_hexdigit())
        && filename == model.file
}

pub fn report_models() -> Result<Vec<ReportModel>, String> {
    let manifest: Manifest = serde_json::from_str(include_str!("../../MODEL_MANIFEST.json"))
        .map_err(|_| "Catalogue des modèles invalide")?;
    let models: Vec<ReportModel> = manifest
        .active_models
        .into_iter()
        .filter(|entry| {
            matches!(
                entry["purpose"].as_str(),
                Some("complete_text_translation_and_report" | "report_only")
            )
        })
        .map(|entry| {
            serde_json::from_value(entry).map_err(|_| "Modèle du catalogue incomplet".to_string())
        })
        .collect::<Result<_, _>>()?;
    let unique_ids: std::collections::BTreeSet<_> = models.iter().map(|m| &m.id).collect();
    let unique_files: std::collections::BTreeSet<_> = models.iter().map(|m| &m.file).collect();
    if models.iter().filter(|m| m.id == "baseline").count() != 1
        || models.iter().any(|m| {
            m.id.is_empty()
                || m.file.contains('/')
                || m.file.contains('\\')
                || m.file == "."
                || m.file == ".."
                || m.bytes == 0
                || m.sha256.len() != 64
                || !m.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                || !pinned_model_url(m)
        })
        || unique_ids.len() != models.len()
        || unique_files.len() != models.len()
    {
        return Err("Catalogue des modèles de compte rendu invalide".into());
    }
    let baseline = models
        .iter()
        .find(|m| m.id == "baseline")
        .ok_or("Modèle de base absent du catalogue")?;
    if baseline.file != MODELE_TEXTE_RECOMMANDE.file_name
        || baseline.bytes != MODELE_TEXTE_RECOMMANDE.size_bytes
        || baseline.sha256 != MODELE_TEXTE_RECOMMANDE.sha256
        || baseline.url != MODELE_TEXTE_RECOMMANDE.url
    {
        return Err("Catalogue et moteur de traduction désynchronisés".into());
    }
    Ok(models)
}

/// Catalogue historique : cette lecture ne donne pas la permission de relancer un modèle.
pub fn report_model(id: &str) -> Result<ReportModel, String> {
    report_models()?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| format!("Modèle de compte rendu inconnu : « {id} »"))
}

/// Liste sélectionnable, distincte des métadonnées des travaux conservés.
pub fn new_report_models() -> Result<Vec<ReportModel>, String> {
    Ok(report_models()?
        .into_iter()
        .filter(|model| model.available_for_new_jobs)
        .collect())
}

/// Garde commun aux téléchargements et à toute nouvelle exécution, reprise comprise.
pub fn report_model_for_generation(id: &str) -> Result<ReportModel, String> {
    let model = report_model(id)?;
    if !model.available_for_new_jobs {
        return Err("La génération avec ce modèle est désactivée. Les fichiers et les comptes rendus déjà conservés restent disponibles.".into());
    }
    Ok(model)
}
