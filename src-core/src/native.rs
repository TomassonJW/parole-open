//! Décodage par tranche et transcription via des exécutables locaux explicites.
use crate::child_process::suppress_child_console;
use crate::diarization::{
    assign_speakers, decode_pcm_f32, DiarizationConfig, Diarizer, SpeakerRegistry, LOCAL_PROTOCOLS,
};
use crate::{process_chunks_observed, Job, Segment};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Deserialize)]
struct WhisperOutput {
    transcription: Vec<WhisperSegment>,
}
#[derive(Deserialize)]
struct WhisperSegment {
    offsets: Offsets,
    text: String,
}
#[derive(Deserialize)]
struct Offsets {
    from: i64,
    to: i64,
}

pub fn parse_whisper_json(bytes: &[u8]) -> Result<Vec<Segment>, String> {
    let parsed: WhisperOutput = serde_json::from_slice(bytes)
        .map_err(|_| "Résultat de transcription illisible".to_string())?;
    parsed
        .transcription
        .into_iter()
        .map(|item| {
            if item.offsets.from < 0 || item.offsets.to < item.offsets.from {
                return Err("Horodatages de transcription invalides".to_string());
            }
            Ok(Segment::new(
                item.offsets.from as u64,
                item.offsets.to as u64,
                item.text.trim().to_string(),
            ))
        })
        .collect()
}

#[derive(Clone)]
pub struct NativeTools {
    pub ffprobe: PathBuf,
    pub ffmpeg: PathBuf,
    pub whisper: PathBuf,
    pub model: PathBuf,
}

fn run(program: &Path, args: &[&std::ffi::OsStr]) -> Result<std::process::Output, String> {
    let mut command = Command::new(program);
    suppress_child_console(&mut command);
    let output = command
        .args(args)
        .output()
        .map_err(|_| format!("Outil local indisponible : {}", program.display()))?;
    if !output.status.success() {
        return Err(format!("Échec d'un outil local ({})", program.display()));
    }
    Ok(output)
}

pub fn probe_duration_ms(media: &Path, ffprobe: &Path) -> Result<u64, String> {
    if !media.is_file() {
        return Err("Fichier introuvable".into());
    }
    let mut command = Command::new(ffprobe);
    suppress_child_console(&mut command);
    let output = command
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            LOCAL_PROTOCOLS,
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(media)
        .output()
        .map_err(|_| "Analyse du média indisponible".to_string())?;
    if !output.status.success() {
        return Err("Média illisible ou format non pris en charge".into());
    }
    let seconds: f64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .map_err(|_| "Durée du média inconnue".to_string())?;
    if !seconds.is_finite() || seconds <= 0.0 || seconds > (u64::MAX as f64 / 1000.0) {
        return Err("Durée du média invalide".into());
    }
    Ok((seconds * 1000.0).ceil() as u64)
}

/// Notifications distinctes : progression sauvegardée et phase de travail courante.
pub struct TranscriptionCallbacks<P, S> {
    pub on_progress: P,
    pub on_step: S,
}

/// Aucun fichier audio complet n'est matérialisé ; les WAV réellement transcrits
/// sont conservés par tranche pour une réécoute locale sur le même repère temporel.
pub fn transcribe_media(
    job: &mut Job,
    state: &Path,
    media: &Path,
    workspace: &Path,
    tools: &NativeTools,
    on_progress: impl FnMut(&Job) -> Result<(), String>,
) -> Result<(), String> {
    transcribe_media_internal(
        job,
        state,
        media,
        workspace,
        tools,
        None,
        TranscriptionCallbacks {
            on_progress,
            on_step: |_, _| {},
        },
    )
}

/// Variante complète : chaque tranche reçoit des locuteurs calculés par inférence locale.
pub fn transcribe_media_diarized(
    job: &mut Job,
    state: &Path,
    media: &Path,
    workspace: &Path,
    tools: &NativeTools,
    diarization: DiarizationConfig,
    on_progress: impl FnMut(&Job) -> Result<(), String>,
) -> Result<(), String> {
    transcribe_media_internal(
        job,
        state,
        media,
        workspace,
        tools,
        Some(diarization),
        TranscriptionCallbacks {
            on_progress,
            on_step: |_, _| {},
        },
    )
}

/// Phases visibles de chaque tranche, sans déclarer du temps comme audio transcrit.
pub fn transcribe_media_diarized_with_steps(
    job: &mut Job,
    state: &Path,
    media: &Path,
    workspace: &Path,
    tools: &NativeTools,
    diarization: DiarizationConfig,
    callbacks: TranscriptionCallbacks<
        impl FnMut(&Job) -> Result<(), String>,
        impl FnMut(usize, &'static str),
    >,
) -> Result<(), String> {
    transcribe_media_internal(
        job,
        state,
        media,
        workspace,
        tools,
        Some(diarization),
        callbacks,
    )
}

fn transcribe_media_internal(
    job: &mut Job,
    state: &Path,
    media: &Path,
    workspace: &Path,
    tools: &NativeTools,
    diarization: Option<DiarizationConfig>,
    callbacks: TranscriptionCallbacks<
        impl FnMut(&Job) -> Result<(), String>,
        impl FnMut(usize, &'static str),
    >,
) -> Result<(), String> {
    let TranscriptionCallbacks {
        mut on_progress,
        mut on_step,
    } = callbacks;
    if !tools.model.is_file() {
        return Err("Modèle de transcription manquant".into());
    }
    fs::create_dir_all(workspace).map_err(|_| "Dossier de travail inaccessible".to_string())?;
    on_step(job.completed_chunks, "préparation des modèles de voix");
    let diarizer = diarization
        .map(Diarizer::new)
        .transpose()
        .map_err(|e| e.to_string())?;
    let registry_path = workspace.join("locuteurs.json");
    let mut registry = if diarizer.is_some() {
        SpeakerRegistry::load(&registry_path).map_err(|e| e.to_string())?
    } else {
        SpeakerRegistry::default()
    };
    let language = job.source_language.clone();
    let work_identity = workspace
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && name.len() <= 255)
        .ok_or("Dossier de travail non identifiable pour la réécoute")?;
    let audio_binding = crate::retained_audio::job_binding(job, work_identity);
    let mut first_segment = job.segments.len();
    process_chunks_observed(
        job,
        state,
        |index, start_ms, duration_ms| {
            on_step(index, "séparation des voix");
            let turns = if let Some(engine) = diarizer.as_ref() {
                let pcm = decode_pcm_f32(&tools.ffmpeg, media, start_ms, Some(duration_ms))
                    .map_err(|e| e.to_string())?;
                let turns = engine
                    .diarize_chunk(&mut registry, index, start_ms, &pcm)
                    .map_err(|e| e.to_string())?;
                registry.save(&registry_path).map_err(|e| e.to_string())?;
                turns
            } else {
                Vec::new()
            };
            let wav = workspace.join(format!("tranche-{index:08}.wav"));
            let output_base = workspace.join(format!("tranche-{index:08}"));
            let start = format!("{:.3}", start_ms as f64 / 1000.0);
            let duration = format!("{:.3}", duration_ms as f64 / 1000.0);
            on_step(index, "préparation audio");
            let mut command = Command::new(&tools.ffmpeg);
            suppress_child_console(&mut command);
            let ffmpeg_output = command
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-protocol_whitelist",
                    LOCAL_PROTOCOLS,
                    "-ss",
                    &start,
                    "-i",
                ])
                .arg(media)
                .args([
                    "-t",
                    &duration,
                    "-vn",
                    "-ac",
                    "1",
                    "-ar",
                    "16000",
                    "-acodec",
                    "pcm_s16le",
                    "-y",
                ])
                .arg(&wav)
                .output()
                .map_err(|_| "Décodeur média indisponible".to_string())?;
            if !ffmpeg_output.status.success() {
                return Err("Décodage du média impossible".into());
            }
            let prepared_audio = crate::retained_audio::prepare(&wav, duration_ms)?;
            on_step(index, "transcription Whisper");
            let result = run(
                &tools.whisper,
                &[
                    "-m".as_ref(),
                    tools.model.as_os_str(),
                    "-f".as_ref(),
                    wav.as_os_str(),
                    "-l".as_ref(),
                    language.as_ref(),
                    "-oj".as_ref(),
                    "-of".as_ref(),
                    output_base.as_os_str(),
                    "-np".as_ref(),
                ],
            )
            .and_then(|_| {
                fs::read(output_base.with_extension("json"))
                    .map_err(|_| "Résultat de transcription absent".to_string())
            })
            .and_then(|bytes| parse_whisper_json(&bytes))
            .and_then(|mut segments| {
                crate::retained_audio::retain_receipt(
                    workspace,
                    index,
                    start_ms,
                    duration_ms,
                    &audio_binding,
                    first_segment,
                    &segments,
                    prepared_audio,
                )?;
                if diarizer.is_some() {
                    let spans: Vec<_> = segments
                        .iter()
                        .map(|s| (s.start_ms + start_ms, s.end_ms + start_ms))
                        .collect();
                    for (segment, speaker) in
                        segments.iter_mut().zip(assign_speakers(&spans, &turns))
                    {
                        segment.speaker_id = speaker;
                    }
                }
                first_segment = first_segment
                    .checked_add(segments.len())
                    .ok_or("Nombre de passages invalide")?;
                Ok(segments)
            });
            // Garder le WAV exact donné à Whisper, y compris après succès.
            // Les échecs restent diagnostiquables ; seule completed_chunks les valide.
            if result.is_ok() {
                let _ = fs::remove_file(output_base.with_extension("json"));
            }
            result
        },
        |job| on_progress(job),
    )?;
    Ok(())
}
