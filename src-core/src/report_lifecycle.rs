//! Préparation commune aux points d'entrée de reprise et d'exécution du rapport.
use crate::{
    finalize_interrupted_job,
    language::{load_completed_report, plan_sections, report_options},
    report_models::report_model_for_generation,
    save_job,
    verified_report::render_synthesis_with_sources,
    Job, Stage,
};
use std::path::Path;

/// Récupère un rapport achevé avant toute exigence de modèle ou nouvelle inférence.
/// Retourne vrai seulement après persistance ; le cache source n'est jamais réécrit.
pub fn prepare_report_execution(job: &mut Job, state_path: &Path) -> Result<bool, String> {
    if !job.generate_report || job.report.is_some() {
        return Ok(false);
    }
    let options = report_options(job)?;
    let directory = state_path
        .parent()
        .ok_or("Dossier du travail introuvable")?;
    if let Some(report) =
        load_completed_report(job, &directory.join("compte-rendu-etat.json"), &options)?
    {
        let mut recovered = job.clone();
        recovered.report = Some(render_synthesis_with_sources(job, &report));
        recovered.report_format_version = 1;
        recovered.phase_total = plan_sections(job, options.section_chars).len() + 1;
        recovered.phase_done = recovered.phase_total;
        recovered.timing.active_since_ms = 0;
        recovered.timing.active_chunk_since_ms = 0;
        recovered.stage = Stage::Interrupted;
        recovered.error =
            Some("Compte rendu récupéré ; une autre étape du traitement reste à terminer.".into());
        if !finalize_interrupted_job(&mut recovered, state_path)
            .map_err(|_| "Impossible de conserver le compte rendu récupéré")?
        {
            save_job(&recovered, state_path)
                .map_err(|_| "Impossible de conserver le compte rendu récupéré")?;
        }
        *job = recovered;
        return Ok(true);
    }
    // Un cache absent, incomplet ou périmé n'autorise jamais un modèle retiré.
    report_model_for_generation(&job.report_model_id)?;
    Ok(false)
}
