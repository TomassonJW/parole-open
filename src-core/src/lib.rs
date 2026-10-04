//! Moteur local de traitement des enregistrements.
pub mod audio_access;
mod audio_access_fs;
mod audio_durability;
pub mod audio_playback;
mod child_process;
pub mod classification_process;
pub mod diarization;
pub mod docx;
pub mod language;
pub mod native;
pub mod report_lifecycle;
pub mod report_models;
pub mod retained_audio;
pub mod topic_access;
pub mod topic_cache;
pub mod topic_candidates;
pub mod topic_classification_access;
pub mod topic_classification_cache;
pub mod topic_questions;
pub mod topic_selection;
pub mod transcript_presentation;
pub mod verified_report;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs, io,
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct JobTiming {
    /// Système sur lequel la mesure a été effectuée ; vide pour les anciens travaux.
    pub platform: String,
    /// Temps local de préparation des modèles/voix avant la transcription des passages.
    pub preparation_ms: u64,
    pub transcription_ms: u64,
    pub translation_ms: u64,
    pub report_ms: u64,
    pub chunk_ms: Vec<u64>,
    pub chunk_audio_ms: Vec<u64>,
    /// Début de la portion en cours ; remis à zéro à la fin ou à l'interruption.
    pub active_since_ms: u64,
    /// Début du passage Whisper courant ; zéro durant la préparation des voix.
    pub active_chunk_since_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    Ready,
    Transcribing,
    Interrupted,
    Translating,
    Reporting,
    Transcribed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    /// Aucun locuteur n'est inventé avant une vraie diarisation.
    pub speaker_id: Option<String>,
    pub translated_text: Option<String>,
}
impl Segment {
    pub fn new(start_ms: u64, end_ms: u64, text: String) -> Self {
        Self {
            start_ms,
            end_ms,
            text,
            speaker_id: None,
            translated_text: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub media_name: String,
    pub duration_ms: u64,
    pub chunk_ms: u64,
    pub completed_chunks: usize,
    pub stage: Stage,
    pub segments: Vec<Segment>,
    pub speaker_names: BTreeMap<String, String>,
    pub error: Option<String>,
    #[serde(default = "default_language")]
    pub source_language: String,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub target_language: Option<String>,
    /// Langue du compte rendu si choisie explicitement ; None conserve le choix historique.
    #[serde(default)]
    pub report_language: Option<String>,
    /// Identifiant épinglé du modèle de compte rendu ; anciens travaux : baseline.
    #[serde(default = "default_report_model_id")]
    pub report_model_id: String,
    #[serde(default)]
    pub generate_report: bool,
    #[serde(default)]
    pub report: Option<String>,
    /// 0 : ancien texte tel quel ; 1 : champs textuels échappés pour Markdown.
    #[serde(default)]
    pub report_format_version: u8,
    #[serde(default)]
    pub phase_done: usize,
    #[serde(default)]
    pub phase_total: usize,
    #[serde(default)]
    pub translation_issues: Vec<usize>,
    #[serde(default)]
    pub timing: JobTiming,
}
fn default_report_model_id() -> String {
    "baseline".into()
}
fn default_language() -> String {
    "auto".into()
}
impl Job {
    pub fn new(media_name: String, duration_ms: u64, chunk_ms: u64) -> Self {
        Self {
            media_name,
            duration_ms,
            chunk_ms,
            completed_chunks: 0,
            stage: Stage::Ready,
            segments: vec![],
            speaker_names: BTreeMap::new(),
            error: None,
            source_language: default_language(),
            source_path: None,
            target_language: None,
            report_language: None,
            report_model_id: default_report_model_id(),
            generate_report: false,
            report: None,
            report_format_version: 0,
            phase_done: 0,
            phase_total: 0,
            translation_issues: vec![],
            timing: JobTiming {
                platform: std::env::consts::OS.into(),
                ..JobTiming::default()
            },
        }
    }
    pub fn translation_incomplete(&self) -> bool {
        self.target_language.is_some()
            && (!self.translation_issues.is_empty()
                || self.segments.iter().any(|s| s.translated_text.is_none()))
    }

    pub fn result_incomplete(&self) -> bool {
        self.stage == Stage::Interrupted
            || self.translation_incomplete()
            || (self.generate_report && self.report.is_none())
    }

    pub fn chunks(&self) -> usize {
        if self.chunk_ms == 0 {
            return 0;
        }
        usize::try_from(
            self.duration_ms / self.chunk_ms
                + u64::from(!self.duration_ms.is_multiple_of(self.chunk_ms)),
        )
        .unwrap_or(usize::MAX)
    }
    pub fn progress(&self) -> f64 {
        if self.stage == Stage::Transcribed {
            return 1.0;
        }
        let spoken = if self.chunks() == 0 {
            0.0
        } else {
            self.completed_chunks as f64 / self.chunks() as f64
        };
        let report_share = if self.generate_report { 0.12 } else { 0.0 };
        let translation_share = if self.target_language.is_some() {
            0.20
        } else {
            0.0
        };
        let speech_share = 1.0 - report_share - translation_share;
        let translation = self
            .segments
            .iter()
            .filter(|s| s.translated_text.is_some())
            .count();
        let translated = if self.segments.is_empty() {
            0.0
        } else {
            translation as f64 / self.segments.len() as f64
        };
        let phase = if self.phase_total == 0 {
            0.0
        } else {
            (self.phase_done as f64 / self.phase_total as f64).min(1.0)
        };
        (speech_share * spoken
            + translation_share * translated
            + if self.stage == Stage::Reporting {
                report_share * phase.min(0.95)
            } else {
                0.0
            })
        .clamp(0.0, 1.0)
    }
}

/// Une tâche encore marquée en cours après redémarrage n'a plus de worker.
pub fn recover_interrupted(job: &mut Job, path: &Path) -> io::Result<bool> {
    let followup_missing = job.stage == Stage::Transcribed
        && (job.translation_incomplete() || (job.generate_report && job.report.is_none()));
    if !matches!(
        job.stage,
        Stage::Transcribing | Stage::Translating | Stage::Reporting
    ) && !followup_missing
    {
        return Ok(false);
    }
    job.stage = Stage::Interrupted;
    job.timing.active_since_ms = 0;
    job.timing.active_chunk_since_ms = 0;
    job.error = Some("Traitement interrompu. Vous pouvez le reprendre.".into());
    save_job(job, path)?;
    Ok(true)
}

/// Finalise sans relancer les modèles un travail dont tous les résultats sont déjà persistés.
/// Une alerte de traduction reste à reprendre, même si le texte du segment existe.
pub fn finalize_interrupted_job(job: &mut Job, path: &Path) -> io::Result<bool> {
    if job.stage != Stage::Interrupted
        || job.completed_chunks < job.chunks()
        || job.chunks() == 0
        || job.translation_incomplete()
        || (job.generate_report && job.report.is_none())
    {
        return Ok(false);
    }
    let mut completed = job.clone();
    completed.stage = Stage::Transcribed;
    completed.error = None;
    completed.timing.active_since_ms = 0;
    completed.timing.active_chunk_since_ms = 0;
    save_job(&completed, path)?;
    *job = completed;
    Ok(true)
}

/// Chaque tranche est persistée par remplacement atomique dans le même dossier.
pub fn save_job(job: &Job, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    let bytes = serde_json::to_vec_pretty(job).map_err(io::Error::other)?;
    let mut file = fs::File::create(&tmp)?;
    use io::Write;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(tmp, path)?;
    Ok(())
}

/// Reprend uniquement les tranches non validées ; le callback n'obtient jamais tout l'audio.
pub fn process_chunks<F>(job: &mut Job, state_path: &Path, transcribe: F) -> Result<(), String>
where
    F: FnMut(usize, u64, u64) -> Result<Vec<Segment>, String>,
{
    process_chunks_observed(job, state_path, transcribe, |_| Ok(()))
}
pub fn process_chunks_observed<F, O>(
    job: &mut Job,
    state_path: &Path,
    mut transcribe: F,
    mut observed: O,
) -> Result<(), String>
where
    F: FnMut(usize, u64, u64) -> Result<Vec<Segment>, String>,
    O: FnMut(&Job) -> Result<(), String>,
{
    if job.chunk_ms == 0 || job.duration_ms == 0 {
        return Err("Durée ou tranche invalide".into());
    }
    if job.completed_chunks > job.chunks() {
        return Err("État de reprise invalide".into());
    }
    job.stage = Stage::Transcribing;
    job.error = None;
    if job.timing.platform.is_empty() && job.timing.chunk_ms.is_empty() {
        job.timing.platform = std::env::consts::OS.into();
    }
    let now = epoch_ms();
    if job.timing.active_since_ms > 0 {
        job.timing.preparation_ms = job
            .timing
            .preparation_ms
            .saturating_add(now.saturating_sub(job.timing.active_since_ms));
    }
    job.timing.active_since_ms = now;
    job.timing.active_chunk_since_ms = now;
    save_job(job, state_path).map_err(|e| e.to_string())?;
    for i in job.completed_chunks..job.chunks() {
        let start = (i as u64).saturating_mul(job.chunk_ms);
        let duration = job.chunk_ms.min(job.duration_ms - start);
        let began = Instant::now();
        match transcribe(i, start, duration) {
            Ok(mut segments) => {
                for seg in &mut segments {
                    if seg.end_ms < seg.start_ms
                        || seg.start_ms > duration.saturating_add(200)
                        || seg.end_ms > duration.saturating_add(200)
                    {
                        let error = "Horodatages de transcription invalides".to_string();
                        job.stage = Stage::Interrupted;
                        job.timing.active_since_ms = 0;
                        job.timing.active_chunk_since_ms = 0;
                        job.error = Some(error.clone());
                        save_job(job, state_path).map_err(|e| e.to_string())?;
                        return Err(error);
                    }
                    seg.start_ms += start;
                    seg.end_ms += start;
                }
                for segment in &segments {
                    if let Some(id) = &segment.speaker_id {
                        job.speaker_names
                            .entry(id.clone())
                            .or_insert_with(|| id.clone());
                    }
                }
                job.segments.append(&mut segments);
                job.completed_chunks = i + 1;
                let elapsed = u64::try_from(began.elapsed().as_millis())
                    .unwrap_or(u64::MAX)
                    .max(1);
                job.timing.chunk_ms.push(elapsed);
                job.timing.chunk_audio_ms.push(duration);
                job.timing.transcription_ms = job.timing.transcription_ms.saturating_add(elapsed);
                job.timing.active_since_ms = epoch_ms();
                job.timing.active_chunk_since_ms = job.timing.active_since_ms;
                save_job(job, state_path).map_err(|e| e.to_string())?;
                if let Err(error) = observed(job) {
                    job.stage = Stage::Interrupted;
                    job.timing.active_since_ms = 0;
                    job.timing.active_chunk_since_ms = 0;
                    job.error = Some(error.clone());
                    save_job(job, state_path).map_err(|e| e.to_string())?;
                    return Err(error);
                }
            }
            Err(error) => {
                job.stage = Stage::Interrupted;
                job.timing.active_since_ms = 0;
                job.timing.active_chunk_since_ms = 0;
                job.error = Some(error.clone());
                save_job(job, state_path).map_err(|e| e.to_string())?;
                return Err(error);
            }
        }
    }
    job.stage = Stage::Transcribed;
    job.timing.active_since_ms = 0;
    job.timing.active_chunk_since_ms = 0;
    save_job(job, state_path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn format_timestamp(ms: u64) -> String {
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        ms % 1000
    )
}
pub fn render_srt(job: &Job) -> String {
    render_subtitles(job, false)
}
pub fn render_vtt(job: &Job) -> String {
    render_subtitles(job, true)
}
fn render_subtitles(job: &Job, vtt: bool) -> String {
    let mut out = if vtt {
        "WEBVTT\n\n".to_string()
    } else {
        String::new()
    };
    for (index, seg) in job.segments.iter().enumerate() {
        if !vtt {
            out.push_str(&format!("{}\n", index + 1));
        }
        let start = format_timestamp(seg.start_ms).replace(',', if vtt { "." } else { "," });
        let end = format_timestamp(seg.end_ms).replace(',', if vtt { "." } else { "," });
        let speaker = seg.speaker_id.as_ref().map(|id| {
            job.speaker_names
                .get(id)
                .map(String::as_str)
                .unwrap_or(id.as_str())
        });
        out.push_str(&format!("{start} --> {end}\n"));
        if let Some(name) = speaker {
            out.push_str(&format!("{name} : "));
        }
        out.push_str(&format!("{}\n\n", seg.text));
    }
    out
}

pub fn render_txt(job: &Job) -> String {
    job.segments
        .iter()
        .map(|segment| {
            let speaker = segment
                .speaker_id
                .as_ref()
                .map(|id| job.speaker_names.get(id).map(String::as_str).unwrap_or(id))
                .unwrap_or("Locuteur non attribué");
            format!(
                "[{}] {} : {}\n",
                format_timestamp(segment.start_ms),
                speaker,
                segment.text
            )
        })
        .collect()
}

fn markdown_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\n' | '\r' => out.push(' '),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            ':' => out.push_str("&#58;"),
            '.' => out.push_str("&#46;"),
            '@' => out.push_str("&#64;"),
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '!'
            | '|' | '~' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

fn render_transcript_markdown(job: &Job, translated: bool) -> String {
    let mut out = String::new();
    for segment in &job.segments {
        let speaker = segment
            .speaker_id
            .as_ref()
            .map(|id| job.speaker_names.get(id).map(String::as_str).unwrap_or(id))
            .unwrap_or("Locuteur non attribué");
        let text = if translated {
            segment
                .translated_text
                .as_deref()
                .unwrap_or("[traduction manquante]")
        } else {
            &segment.text
        };
        out.push_str(&format!(
            "[{}] {} : {}\n",
            format_timestamp(segment.start_ms),
            markdown_literal(speaker),
            markdown_literal(text)
        ));
    }
    out
}

fn report_markdown_without_active_links(report: &str) -> String {
    let mut out = String::with_capacity(report.len());
    for ch in report.chars() {
        out.push_str(match ch {
            '<' => "&lt;",
            '!' => "&#33;",
            '[' => "&#91;",
            ']' => "&#93;",
            '(' => "&#40;",
            ')' => "&#41;",
            ':' => "&#58;",
            '.' => "&#46;",
            '@' => "&#64;",
            _ => {
                out.push(ch);
                continue;
            }
        });
    }
    out
}

/// Export partageable : conserve le contenu, mais pas le chemin interne du média.
/// Le chemin reste présent uniquement dans travail.json afin de permettre la reprise.
pub fn render_export_json(job: &Job) -> serde_json::Result<Vec<u8>> {
    let mut exported = job.clone();
    exported.source_path = None;
    serde_json::to_vec_pretty(&exported)
}

/// Les formats incomplets sans avertissement intrinsèque ne quittent pas l'application.
pub fn export_format_allowed(job: &Job, format: &str) -> bool {
    if !matches!(job.stage, Stage::Interrupted | Stage::Transcribed) {
        return false;
    }
    match format {
        "txt" | "md" | "docx" => true,
        "json" | "srt" | "vtt" => !job.result_incomplete(),
        _ => false,
    }
}

pub const INTERRUPTED_EXPORT_TITLE: &str = "Résultat incomplet - vérification nécessaire";
pub const INTERRUPTED_EXPORT_WARNING: &str = "Les paroles peuvent être partielles, la traduction inachevée et le compte rendu absent ou provisoire. Reprendre le traitement et vérifier ce fichier avant toute diffusion.";

pub fn render_complete_markdown(job: &Job) -> String {
    let mut out = String::new();
    if job.result_incomplete() {
        out.push_str(&format!(
            "> **{INTERRUPTED_EXPORT_TITLE}**\n>\n> {INTERRUPTED_EXPORT_WARNING}\n\n"
        ));
    }
    out.push_str(&format!(
        "# {}\n\n## Transcription originale\n\n",
        markdown_literal(&job.media_name)
    ));
    out.push_str(&render_transcript_markdown(job, false));
    if job.target_language.is_some() {
        out.push_str("\n## Traduction\n\n");
        out.push_str(&render_transcript_markdown(job, true));
    }
    if let Some(report) = &job.report {
        out.push_str("\n## Compte rendu\n\n");
        if job.report_format_version == 1 {
            // Les titres/listes restent structurés ; les liens, images et balises
            // éventuels du modèle sont traités comme du texte dans l'export.
            out.push_str(&report_markdown_without_active_links(report));
        } else {
            // Un ancien rapport n'identifie pas ses passages non fiables :
            // aucune ligne de ce texte ne peut donc définir une section ou un lien.
            out.push_str("Ancien compte rendu en texte brut - actualisez-le pour retrouver sa mise en forme.\n\n");
            for line in report.split('\n') {
                out.push_str("    ");
                out.push_str(&line.replace('\r', " "));
                out.push('\n');
            }
        }
    }
    out
}

pub fn render_complete_txt(job: &Job) -> String {
    let mut out = String::new();
    if job.result_incomplete() {
        out.push_str(&format!(
            "{INTERRUPTED_EXPORT_TITLE}\n{INTERRUPTED_EXPORT_WARNING}\n\n"
        ));
    }
    out.push_str(&format!("Transcription originale - {}\n\n", job.media_name));
    out.push_str(&render_txt(job));
    if job.target_language.is_some() {
        out.push_str("\nTraduction\n\n");
        out.push_str(&language::render_translated_txt(job));
    }
    if let Some(report) = &job.report {
        out.push_str("\nCompte rendu\n\n");
        if job.report_format_version == 1 {
            out.push_str(&verified_report::decode_encoded_report_text(report));
        } else {
            out.push_str(report);
        }
    }
    out
}
