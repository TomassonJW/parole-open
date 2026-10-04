use crate::models;
use parole_core::{
    diarization::DiarizationConfig,
    epoch_ms, finalize_interrupted_job,
    language::{
        build_report, guess_language, load_report_state,
        may_build_source_report_after_translation_error, report_options, translate_job,
        validate_report_choice, validate_report_input, Language, LlamaCppEngine,
        TranslationOptions, MODELE_TEXTE_RECOMMANDE,
    },
    native::{
        probe_duration_ms, transcribe_media_diarized_with_steps, NativeTools,
        TranscriptionCallbacks,
    },
    recover_interrupted,
    report_lifecycle::prepare_report_execution,
    save_job,
    verified_report::render_synthesis_with_sources,
    Job, Stage,
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
pub struct RunState {
    busy: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    active_id: Arc<Mutex<Option<String>>>,
}
impl RunState {
    pub(crate) fn with_idle<R>(
        &self,
        action: impl FnOnce() -> Result<R, String>,
    ) -> Result<R, String> {
        let _active = self.active_id.lock().unwrap_or_else(|p| p.into_inner());
        if self.busy.load(Ordering::SeqCst) {
            return Err("Attendez la fin du traitement avant de renommer les locuteurs".into());
        }
        action()
    }
}

fn reserve(state: &RunState, id: &str) -> Result<(), String> {
    let mut active = state.active_id.lock().unwrap_or_else(|p| p.into_inner());
    if state
        .busy
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("Un autre enregistrement est déjà en cours de traitement".into());
    }
    state.cancel.store(false, Ordering::SeqCst);
    *active = Some(id.to_string());
    Ok(())
}

fn release(state: &RunState) {
    let mut active = state.active_id.lock().unwrap_or_else(|p| p.into_inner());
    *active = None;
    state.busy.store(false, Ordering::SeqCst);
}

#[derive(Clone, Serialize)]
pub struct JobView {
    pub id: String,
    pub job: Job,
}

#[derive(Clone, Serialize)]
struct JobActivity {
    id: String,
    phase: &'static str,
    chunk: usize,
    total: usize,
}

pub(crate) fn jobs_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|_| "Dossier de données inaccessible")?
        .join("jobs"))
}
pub(crate) fn folder(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "Identifiant de travail invalide")?;
    Ok(jobs_dir(app)?.join(id))
}
pub fn load(app: &AppHandle, id: &str) -> Result<Job, String> {
    let bytes =
        fs::read(folder(app, id)?.join("travail.json")).map_err(|_| "Travail introuvable")?;
    serde_json::from_slice(&bytes).map_err(|_| "Travail endommagé".into())
}

pub fn recover_jobs(app: &AppHandle) -> Result<(), String> {
    let root = jobs_dir(app)?;
    if !root.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(root)
        .map_err(|_| "Liste des travaux inaccessible")?
        .flatten()
    {
        let id = entry.file_name().to_string_lossy().into_owned();
        if let Ok(mut job) = load(app, &id) {
            recover_interrupted(&mut job, &entry.path().join("travail.json"))
                .map_err(|_| "Impossible de conserver la reprise d'un travail")?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn list_jobs(app: AppHandle) -> Result<Vec<JobView>, String> {
    let root = jobs_dir(&app)?;
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let mut jobs = Vec::new();
    for entry in fs::read_dir(root)
        .map_err(|_| "Liste des travaux inaccessible")?
        .flatten()
    {
        let id = entry.file_name().to_string_lossy().into_owned();
        if let Ok(job) = load(&app, &id) {
            jobs.push(JobView { id, job });
        }
    }
    jobs.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(jobs)
}

#[tauri::command]
pub fn start_job(
    app: AppHandle,
    state: State<'_, RunState>,
    media_path: String,
    source_language: String,
    target_language: Option<String>,
    report_language: Option<String>,
    report_model_id: String,
    generate_report: bool,
) -> Result<JobView, String> {
    models::report_model(&report_model_id)?;
    if !generate_report && report_model_id != "baseline" {
        return Err("Choisissez un modèle de compte rendu seulement si celui-ci est activé".into());
    }
    if generate_report {
        models::report_model_path(&app, &report_model_id)?;
    }
    let report_language = report_language
        .as_deref()
        .map(Language::parse)
        .transpose()?
        .map(|language| language.code().to_string());
    if report_language.is_some() && !generate_report {
        return Err("Choisissez la langue du compte rendu seulement si celui-ci est activé".into());
    }
    let target = target_language
        .as_deref()
        .map(parole_core::language::Language::parse)
        .transpose()?
        .map(|language| language.code().to_string());
    validate_report_choice(
        &source_language,
        target.as_deref(),
        report_language.as_deref(),
    )?;
    if (target.is_some() || generate_report)
        && !["auto", "fr", "en"].contains(&source_language.as_str())
    {
        return Err("Traduction et compte rendu exigent une source française ou anglaise.".into());
    }
    if !["auto", "fr", "en", "es", "de", "it", "pt", "nl"].contains(&source_language.as_str()) {
        return Err("Langue source non prise en charge".into());
    }
    let needs_baseline = target.is_some() || (generate_report && report_model_id == "baseline");
    if !models::model_status(app.clone())?.ready_for(needs_baseline) {
        return Err(
            "Préparez les modèles nécessaires et les moteurs locaux avant de démarrer.".into(),
        );
    }
    let media = PathBuf::from(&media_path);
    if !media.is_file() {
        return Err("Fichier audio ou vidéo introuvable".into());
    }
    let tools = NativeTools {
        ffmpeg: models::native_path(&app, "ffmpeg")?,
        ffprobe: models::native_path(&app, "ffprobe")?,
        whisper: models::native_path(&app, "whisper-cli")?,
        model: models::model_path(&app)?,
    };
    let duration = probe_duration_ms(&media, &tools.ffprobe)?;
    let mut job = Job::new(
        media
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        duration,
        30_000,
    );
    job.source_language = source_language;
    job.source_path = Some(media_path);
    job.target_language = target;
    job.report_language = report_language;
    job.report_model_id = report_model_id;
    job.generate_report = generate_report;
    job.stage = Stage::Transcribing;
    job.timing.active_since_ms = epoch_ms();
    let id = uuid::Uuid::new_v4().to_string();
    let diarization = models::diarization_config(&app)?;
    reserve(state.inner(), &id)?;
    let result = (|| {
        let directory = folder(&app, &id)?;
        let presentation = app.state::<crate::presentation_store::Store>();
        presentation.create_job_dir(&id)?;
        presentation.snapshot_new_job(&id)?;
        save_job(&job, &directory.join("travail.json"))
            .map_err(|_| "Impossible de conserver le travail")?;
        let view = JobView { id, job };
        launch(app, state.inner(), view.clone(), media, tools, diarization);
        Ok(view)
    })();
    if result.is_err() {
        release(state.inner());
    }
    result
}

#[tauri::command]
pub fn resume_job(
    app: AppHandle,
    state: State<'_, RunState>,
    id: String,
) -> Result<JobView, String> {
    let mut job = load(&app, &id)?;
    if state
        .active_id
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_deref()
        == Some(id.as_str())
    {
        return Ok(JobView { id, job });
    }
    let recovered_report = state.with_idle(|| {
        let path = folder(&app, &id)?.join("travail.json");
        prepare_report_execution(&mut job, &path)
    })?;
    if recovered_report && job.stage == Stage::Transcribed {
        return Ok(JobView { id, job });
    }
    if job.stage == Stage::Interrupted {
        reserve(state.inner(), &id)?;
        let finalized = folder(&app, &id).and_then(|directory| {
            finalize_interrupted_job(&mut job, &directory.join("travail.json"))
                .map_err(|_| "Impossible de finaliser ce travail".into())
        });
        release(state.inner());
        if finalized? {
            return Ok(JobView { id, job });
        }
    }
    models::report_model(&job.report_model_id)?;
    if job.generate_report && job.report.is_none() {
        models::report_model_path(&app, &job.report_model_id)?;
    }
    if job.stage == Stage::Transcribed
        && (!job.generate_report || job.report.is_some())
        && (job.target_language.is_none()
            || (job.translation_issues.is_empty()
                && job.segments.iter().all(|s| s.translated_text.is_some())))
    {
        return Err("Ce travail est déjà terminé.".into());
    }
    if job.completed_chunks >= job.chunks()
        && (job.target_language.is_none()
            || (job.translation_issues.is_empty()
                && job.segments.iter().all(|s| s.translated_text.is_some())))
        && (!job.generate_report || job.report.is_some())
    {
        return Err("Aucune étape restante à reprendre.".into());
    }
    let media = PathBuf::from(
        job.source_path
            .as_ref()
            .ok_or("Fichier source non enregistré")?,
    );
    if !media.is_file() {
        return Err(
            "Le fichier source a été déplacé. Remettez-le à son emplacement initial.".into(),
        );
    }
    let needs_baseline = job.target_language.is_some()
        || (job.generate_report && job.report.is_none() && job.report_model_id == "baseline");
    if !models::model_status(app.clone())?.ready_for(needs_baseline) {
        return Err("Modèles ou moteurs locaux nécessaires manquants".into());
    }
    let tools = NativeTools {
        ffmpeg: models::native_path(&app, "ffmpeg")?,
        ffprobe: models::native_path(&app, "ffprobe")?,
        whisper: models::native_path(&app, "whisper-cli")?,
        model: models::model_path(&app)?,
    };
    let diarization = models::diarization_config(&app)?;
    reserve(state.inner(), &id)?;
    if job.completed_chunks < job.chunks() {
        job.stage = Stage::Transcribing;
    } else if job.target_language.is_some()
        && (!job.translation_issues.is_empty()
            || job.segments.iter().any(|s| s.translated_text.is_none()))
    {
        job.stage = Stage::Translating;
    } else {
        job.stage = Stage::Reporting;
    }
    job.error = None;
    if job.timing.platform.is_empty()
        && job.timing.chunk_ms.is_empty()
        && job.timing.translation_ms == 0
        && job.timing.report_ms == 0
    {
        job.timing.platform = std::env::consts::OS.into();
    }
    job.timing.active_since_ms = epoch_ms();
    job.timing.active_chunk_since_ms = 0;
    if save_job(&job, &folder(&app, &id)?.join("travail.json")).is_err() {
        release(state.inner());
        return Err("Impossible de reprendre ce travail".into());
    }
    let view = JobView { id, job };
    launch(app, state.inner(), view.clone(), media, tools, diarization);
    Ok(view)
}

fn launch(
    app: AppHandle,
    state: &RunState,
    view: JobView,
    media: PathBuf,
    tools: NativeTools,
    diarization: DiarizationConfig,
) {
    let busy = state.busy.clone();
    let cancel = state.cancel.clone();
    let active_id = state.active_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut job = view.job;
        let id = view.id;
        let directory = folder(&app, &id).expect("Identifiant de travail interne valide");
        let total_chunks = job.chunks();
        let result = transcribe_media_diarized_with_steps(
            &mut job,
            &directory.join("travail.json"),
            &media,
            &directory,
            &tools,
            diarization,
            TranscriptionCallbacks {
                on_progress: |snapshot: &Job| {
                    let mut visible = snapshot.clone();
                    if visible.stage == Stage::Transcribed {
                        if visible.target_language.is_some() {
                            visible.stage = Stage::Translating;
                        } else if visible.generate_report {
                            visible.stage = Stage::Reporting;
                        }
                    }
                    let _ = app.emit(
                        "job-progress",
                        JobView {
                            id: id.clone(),
                            job: visible,
                        },
                    );
                    if cancel.load(Ordering::SeqCst) {
                        Err("Traitement interrompu. Vous pouvez le reprendre.".into())
                    } else {
                        Ok(())
                    }
                },
                on_step: |chunk, phase| {
                    let _ = app.emit(
                        "job-activity",
                        JobActivity {
                            id: id.clone(),
                            phase,
                            chunk,
                            total: total_chunks,
                        },
                    );
                },
            },
        );
        let result = result.and_then(|_| {
            match run_translation(&app, &id, &directory, &mut job, &cancel) {
                Ok(()) => run_report(&app, &id, &directory, &mut job, &cancel),
                Err(translation_error) => {
                    if may_build_source_report_after_translation_error(
                        &job,
                        cancel.load(Ordering::SeqCst),
                    ) && save_job(&job, &directory.join("travail.json")).is_ok()
                    {
                        // La langue du rapport a été demandée explicitement comme
                        // celle des paroles ; aucune traduction absente n'y entre.
                        match run_report(&app, &id, &directory, &mut job, &cancel) {
                            Ok(()) => Err(format!(
                                "Traduction non terminée : {translation_error}. Le compte rendu fondé sur les paroles originales est conservé ; reprenez la traduction séparément."
                            )),
                            Err(report_error) => Err(format!(
                                "Traduction non terminée : {translation_error}. Compte rendu non produit : {report_error}"
                            )),
                        }
                    } else {
                        Err(translation_error)
                    }
                }
            }
        });
        if let Err(error) = result {
            job.stage = Stage::Interrupted;
            job.timing.active_since_ms = 0;
            job.timing.active_chunk_since_ms = 0;
            job.error = Some(error);
            let _ = save_job(&job, &directory.join("travail.json"));
        }
        let _ = app.emit("job-progress", JobView { id, job });
        let mut active = active_id.lock().unwrap_or_else(|p| p.into_inner());
        *active = None;
        busy.store(false, Ordering::SeqCst);
    });
}

fn run_translation(
    app: &AppHandle,
    id: &str,
    directory: &Path,
    job: &mut Job,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let Some(target_code) = job.target_language.as_deref() else {
        return Ok(());
    };
    let target = Language::parse(target_code)?;
    let source = Language::parse(&job.source_language)
        .ok()
        .or_else(|| {
            let sample = job
                .segments
                .iter()
                .take(80)
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            guess_language(&sample)
        })
        .ok_or("Langue source inconnue : choisissez le français ou l'anglais avant de reprendre")?;
    let state_path = directory.join("travail.json");
    job.stage = Stage::Translating;
    job.error = None;
    job.phase_total = job.segments.len();
    job.phase_done = job
        .segments
        .iter()
        .enumerate()
        .filter(|(index, s)| s.translated_text.is_some() && !job.translation_issues.contains(index))
        .count();
    job.timing.active_since_ms = epoch_ms();
    let began = Instant::now();
    save_job(job, &state_path).map_err(|_| "Impossible de conserver l'étape de traduction")?;
    let _ = app.emit(
        "job-progress",
        JobView {
            id: id.to_string(),
            job: job.clone(),
        },
    );
    if source == target {
        job.translation_issues.clear();
        for s in &mut job.segments {
            s.translated_text = Some(s.text.clone());
        }
    } else {
        let binary = models::native_path(app, "llama-completion")?;
        let model = models::text_model_path(app)?;
        let mut engine = LlamaCppEngine::new(binary, model, directory.to_path_buf());
        engine.check(&MODELE_TEXTE_RECOMMANDE)?;
        let options = TranslationOptions::new(target);
        let translated = translate_job(job, &state_path, &mut engine, &options, |done, total| {
            let mut current = load(app, id)?;
            current.stage = Stage::Translating;
            current.phase_done = done;
            current.phase_total = total;
            let _ = app.emit(
                "job-progress",
                JobView {
                    id: id.to_string(),
                    job: current,
                },
            );
            if cancel.load(Ordering::SeqCst) {
                Err("Traitement interrompu. Vous pouvez le reprendre.".into())
            } else {
                Ok(())
            }
        });
        job.timing.translation_ms = job
            .timing
            .translation_ms
            .saturating_add(u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX));
        job.timing.active_since_ms = 0;
        translated?;
        // Les alertes sont persistées dans chaque lot par le moteur.
    }
    if source == target {
        job.timing.translation_ms = job
            .timing
            .translation_ms
            .saturating_add(u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX));
        job.timing.active_since_ms = 0;
    }
    job.phase_done = if job.generate_report {
        0
    } else {
        job.phase_total
    };
    if job.generate_report && job.report.is_none() {
        job.phase_total = 0;
    }
    if job.translation_incomplete() {
        job.error =
            Some("Certains passages traduits restent à vérifier : reprenez la traduction.".into());
    }
    job.stage = if job.generate_report && job.report.is_none() {
        Stage::Reporting
    } else if job.translation_incomplete() {
        Stage::Interrupted
    } else {
        Stage::Transcribed
    };
    save_job(job, &state_path).map_err(|_| "Impossible de conserver la traduction")?;
    Ok(())
}

fn run_report(
    app: &AppHandle,
    id: &str,
    directory: &Path,
    job: &mut Job,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if !job.generate_report || job.report.is_some() {
        return Ok(());
    }
    if prepare_report_execution(job, &directory.join("travail.json"))? {
        let _ = app.emit(
            "job-progress",
            JobView {
                id: id.to_string(),
                job: job.clone(),
            },
        );
        return Ok(());
    }
    let options = report_options(job)?;
    validate_report_input(job, &options)?;
    let state_path = directory.join("travail.json");
    job.stage = Stage::Reporting;
    job.phase_total = parole_core::language::plan_sections(job, options.section_chars).len() + 1;
    job.phase_done = load_report_state(&directory.join("compte-rendu-etat.json"))
        .ok()
        .flatten()
        .filter(|state| {
            state.language == options.language && state.use_translation == options.use_translation
        })
        .map(|state| {
            if state.report.is_some() {
                job.phase_total
            } else {
                state.sections.len()
            }
        })
        .unwrap_or(0)
        .min(job.phase_total);
    job.error = None;
    job.timing.active_since_ms = epoch_ms();
    let mut step_started = Instant::now();
    save_job(job, &state_path).map_err(|_| "Impossible de conserver l'étape de compte rendu")?;
    let _ = app.emit(
        "job-progress",
        JobView {
            id: id.to_string(),
            job: job.clone(),
        },
    );

    let (report_path, report_model) = models::report_model_path(app, &job.report_model_id)?;
    let mut engine = LlamaCppEngine::new(
        models::native_path(app, "llama-completion")?,
        report_path,
        directory.to_path_buf(),
    );
    engine.check(&report_model.spec())?;
    let report_result = build_report(
        job,
        &directory.join("compte-rendu-etat.json"),
        &mut engine,
        &options,
        |done, total| {
            let mut current = load(app, id)?;
            current.timing.report_ms = current.timing.report_ms.saturating_add(
                u64::try_from(step_started.elapsed().as_millis()).unwrap_or(u64::MAX),
            );
            step_started = Instant::now();
            current.timing.active_since_ms = epoch_ms();
            current.stage = Stage::Reporting;
            current.phase_done = done;
            current.phase_total = total;
            save_job(&current, &state_path)
                .map_err(|_| "Impossible de conserver la progression du compte rendu")?;
            let _ = app.emit(
                "job-progress",
                JobView {
                    id: id.to_string(),
                    job: current,
                },
            );
            if cancel.load(Ordering::SeqCst) {
                Err("Traitement interrompu. Vous pouvez le reprendre.".into())
            } else {
                Ok(())
            }
        },
    );
    let persisted = load(app, id)?;
    job.phase_total = persisted.phase_total;
    job.phase_done = persisted.phase_done;
    job.timing.report_ms = persisted
        .timing
        .report_ms
        .saturating_add(u64::try_from(step_started.elapsed().as_millis()).unwrap_or(u64::MAX));
    job.timing.active_since_ms = 0;
    let report = report_result?;

    job.report = Some(render_synthesis_with_sources(job, &report));
    job.report_format_version = 1;
    job.phase_done = job.phase_total;
    if job.translation_incomplete() {
        // Même en cas de fermeture entre cette sauvegarde et la gestion de
        // l'erreur, la traduction reste explicitement reprenable.
        job.stage = Stage::Interrupted;
        job.error = Some(
            "Traduction incomplète ou douteuse : reprenez le traitement pour la vérifier.".into(),
        );
    } else {
        job.stage = Stage::Transcribed;
    }
    save_job(job, &state_path).map_err(|_| "Impossible de conserver le compte rendu")?;
    Ok(())
}

#[tauri::command]
pub fn cancel_job(state: State<'_, RunState>, id: String) -> Result<(), String> {
    if state
        .active_id
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_deref()
        != Some(id.as_str())
    {
        return Err("Ce travail n'est pas en cours".into());
    }
    state.cancel.store(true, Ordering::SeqCst);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_work_is_rejected_before_its_persisted_creation() {
        let state = RunState::default();
        reserve(&state, "first").unwrap();
        assert!(reserve(&state, "second").is_err());
        assert_eq!(state.active_id.lock().unwrap().as_deref(), Some("first"));
        assert!(state.with_idle(|| Ok(())).is_err());
        release(&state);
        assert!(state.with_idle(|| Ok(())).is_ok());
        reserve(&state, "second").unwrap();
        release(&state);
    }
}
