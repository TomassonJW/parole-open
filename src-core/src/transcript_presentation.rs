//! Projection et exports de lecture : aucune modification de la transcription source.
use crate::{Job, Segment, INTERRUPTED_EXPORT_TITLE, INTERRUPTED_EXPORT_WARNING};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    Fluid,
    Detailed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewOptions {
    pub mode: ViewMode,
    pub pause_ms: u32,
    pub show_timestamps: bool,
}
impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            mode: ViewMode::Fluid,
            pause_ms: 2000,
            show_timestamps: true,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportContent {
    Complete,
    Transcript,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportOptions {
    pub linked: bool,
    pub view: ViewOptions,
    pub content: ExportContent,
}
impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            linked: true,
            view: ViewOptions::default(),
            content: ExportContent::Complete,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationPreferences {
    pub schema_version: u32,
    pub screen: ViewOptions,
    pub export: ExportOptions,
    pub speaker_colors: BTreeMap<String, String>,
}
impl Default for PresentationPreferences {
    fn default() -> Self {
        Self {
            schema_version: 1,
            screen: ViewOptions::default(),
            export: ExportOptions::default(),
            speaker_colors: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationState {
    pub preferences: PresentationPreferences,
    pub revision: u64,
    pub warning: Option<String>,
    pub writable: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptBlock {
    pub segment_indices: Vec<usize>,
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationSnapshot {
    pub schema_version: u32,
    pub job_id: String,
    pub source_revision: String,
    pub options: ViewOptions,
    pub blocks: Vec<TranscriptBlock>,
}
pub fn validate_view(options: &ViewOptions) -> Result<(), String> {
    if options.pause_ms > 10_000 || !options.pause_ms.is_multiple_of(500) {
        return Err("La pause doit être entre 0 et 10000 ms, par pas de 500 ms".into());
    }
    Ok(())
}
pub fn validate_preferences(p: &PresentationPreferences) -> Result<(), String> {
    if p.schema_version != 1 {
        return Err("Version de présentation inconnue".into());
    }
    validate_view(&p.screen)?;
    validate_view(&p.export.view)?;
    for (id, color) in &p.speaker_colors {
        if id.trim().is_empty() || id.len() > 128 || id.chars().any(char::is_control) {
            return Err("Identifiant de locuteur invalide".into());
        }
        if !valid_color(color) {
            return Err("Couleur de locuteur invalide : #RRGGBB requis".into());
        }
    }
    Ok(())
}
fn valid_color(color: &str) -> bool {
    color.len() == 7
        && color.starts_with('#')
        && color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}
/// Validation documentaire à appliquer aussi lors de la sauvegarde des préférences.
pub fn validate_preferences_for_job(p: &PresentationPreferences, job: &Job) -> Result<(), String> {
    validate_preferences(p)?;
    let ids: HashSet<&str> = job
        .segments
        .iter()
        .filter_map(|s| s.speaker_id.as_deref())
        .collect();
    if p.speaker_colors.keys().any(|id| !ids.contains(id.as_str())) {
        return Err("Couleur attribuée à un locuteur absent du document".into());
    }
    Ok(())
}

pub fn group_segments(
    segments: &[Segment],
    options: &ViewOptions,
) -> Result<Vec<TranscriptBlock>, String> {
    validate_view(options)?;
    let mut blocks: Vec<TranscriptBlock> = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        let previous = index.checked_sub(1).and_then(|i| segments.get(i));
        let can_join = options.mode == ViewMode::Fluid
            && previous.is_some_and(|prev| {
                let known = segment
                    .speaker_id
                    .as_deref()
                    .filter(|s| !s.trim().is_empty());
                known.is_some()
                    && known == prev.speaker_id.as_deref()
                    && prev.start_ms <= prev.end_ms
                    && segment.start_ms <= segment.end_ms
                    && prev.end_ms <= segment.start_ms
                    && segment.start_ms - prev.end_ms <= u64::from(options.pause_ms)
            });
        if can_join {
            let block = blocks.last_mut().expect("previous segment has a block");
            block.segment_indices.push(index);
            block.end_ms = segment.end_ms;
        } else {
            blocks.push(TranscriptBlock {
                segment_indices: vec![index],
                start_ms: segment.start_ms,
                end_ms: segment.end_ms,
                speaker_id: segment.speaker_id.clone(),
            });
        }
    }
    Ok(blocks)
}
pub fn source_revision(segments: &[Segment]) -> Result<String, String> {
    let tuples: Vec<_> = segments
        .iter()
        .map(|s| {
            (
                s.start_ms,
                s.end_ms,
                &s.text,
                &s.speaker_id,
                &s.translated_text,
            )
        })
        .collect();
    let bytes = serde_json::to_vec(&tuples)
        .map_err(|e| format!("Révision de transcription impossible : {e}"))?;
    Ok(crate::language::sha256_hex(&bytes))
}
pub fn project(
    job_id: &str,
    job: &Job,
    options: &ViewOptions,
) -> Result<PresentationSnapshot, String> {
    Ok(PresentationSnapshot {
        schema_version: 1,
        job_id: job_id.into(),
        source_revision: source_revision(&job.segments)?,
        options: options.clone(),
        blocks: group_segments(&job.segments, options)?,
    })
}
pub(crate) fn export_blocks(
    job: &Job,
    p: &PresentationPreferences,
) -> Result<Vec<TranscriptBlock>, String> {
    validate_preferences_for_job(p, job)?;
    group_segments(
        &job.segments,
        if p.export.linked {
            &p.screen
        } else {
            &p.export.view
        },
    )
}
pub(crate) fn show_export_timestamps(p: &PresentationPreferences) -> bool {
    (if p.export.linked {
        &p.screen
    } else {
        &p.export.view
    })
    .show_timestamps
}
pub(crate) fn block_speaker<'a>(job: &'a Job, block: &'a TranscriptBlock) -> &'a str {
    block
        .speaker_id
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|id| job.speaker_names.get(id).map(String::as_str).unwrap_or(id))
        .unwrap_or("Locuteur non attribué")
}
pub(crate) fn block_text(job: &Job, block: &TranscriptBlock, translated: bool) -> String {
    block
        .segment_indices
        .iter()
        .map(|&i| {
            if translated {
                job.segments[i]
                    .translated_text
                    .as_deref()
                    .unwrap_or("[traduction manquante]")
            } else {
                &job.segments[i].text
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn markdown_literal(text: &str) -> String {
    let mut out = String::new();
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
fn report_markdown_without_active_links(report: &str) -> String {
    let mut out = String::new();
    for ch in report.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '!' => out.push_str("&#33;"),
            '[' => out.push_str("&#91;"),
            ']' => out.push_str("&#93;"),
            '(' => out.push_str("&#40;"),
            ')' => out.push_str("&#41;"),
            ':' => out.push_str("&#58;"),
            '.' => out.push_str("&#46;"),
            '@' => out.push_str("&#64;"),
            _ => out.push(ch),
        }
    }
    out
}
fn render(job: &Job, p: &PresentationPreferences, markdown: bool) -> Result<String, String> {
    let blocks = export_blocks(job, p)?;
    let mut out = String::new();
    if job.result_incomplete() {
        if markdown {
            out.push_str(&format!(
                "> **{INTERRUPTED_EXPORT_TITLE}**\n>\n> {INTERRUPTED_EXPORT_WARNING}\n\n"
            ));
        } else {
            out.push_str(&format!(
                "{INTERRUPTED_EXPORT_TITLE}\n{INTERRUPTED_EXPORT_WARNING}\n\n"
            ));
        }
    }
    if markdown {
        out.push_str(&format!(
            "# {}\n\n## Transcription originale\n\n",
            markdown_literal(&job.media_name)
        ));
    } else {
        out.push_str(&format!("Transcription originale - {}\n\n", job.media_name));
    }
    let sections = if p.export.content == ExportContent::Complete && job.target_language.is_some() {
        2
    } else {
        1
    };
    for section in 0..sections {
        if section == 1 {
            out.push_str(if markdown {
                "\n## Traduction\n\n"
            } else {
                "\nTraduction\n\n"
            });
        }
        for block in &blocks {
            let timestamp = if show_export_timestamps(p) {
                format!("[{}] ", crate::format_timestamp(block.start_ms))
            } else {
                String::new()
            };
            let speaker = block_speaker(job, block);
            let text = block_text(job, block, section == 1);
            if markdown {
                out.push_str(&format!(
                    "{}**{}** : {}\n\n",
                    timestamp,
                    markdown_literal(speaker),
                    markdown_literal(&text)
                ));
            } else {
                out.push_str(&format!("{timestamp}{speaker} : {text}\n"));
            }
        }
    }
    if p.export.content == ExportContent::Complete {
        if let Some(report) = &job.report {
            out.push_str(if markdown {
                "\n## Compte rendu\n\n"
            } else {
                "\nCompte rendu\n\n"
            });
            if job.report_format_version == 1 {
                let decoded = crate::verified_report::decode_encoded_report_text(report);
                if markdown {
                    out.push_str(&report_markdown_without_active_links(report));
                } else {
                    out.push_str(&decoded);
                }
            } else if markdown {
                out.push_str("Ancien compte rendu en texte brut - actualisez-le pour retrouver sa mise en forme.\n\n");
                for line in report.split('\n') {
                    out.push_str("    ");
                    out.push_str(&line.replace('\r', " "));
                    out.push('\n');
                }
            } else {
                out.push_str(report);
            }
        }
    }
    Ok(out)
}
pub fn render_presented_txt(
    job: &Job,
    preferences: &PresentationPreferences,
) -> Result<String, String> {
    render(job, preferences, false)
}
pub fn render_presented_markdown(
    job: &Job,
    preferences: &PresentationPreferences,
) -> Result<String, String> {
    render(job, preferences, true)
}
