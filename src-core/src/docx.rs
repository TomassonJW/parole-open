//! Export DOCX local, sans conversion externe.
use crate::{
    transcript_presentation::{self as presentation, ExportContent, PresentationPreferences},
    Job, INTERRUPTED_EXPORT_TITLE, INTERRUPTED_EXPORT_WARNING,
};
use std::io::{Cursor, Write};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

fn paragraph(out: &mut String, text: &str, heading: bool) {
    out.push_str("<w:p>");
    if heading {
        out.push_str("<w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr>");
    }
    out.push_str("<w:r><w:t xml:space=\"preserve\">");
    out.push_str(&xml(text));
    out.push_str("</w:t></w:r></w:p>");
}

fn xml(text: &str) -> String {
    text.chars()
        .filter(|c| matches!(*c, '\t' | '\n' | '\r') || *c >= ' ')
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&apos;".into(),
            _ => c.to_string(),
        })
        .collect()
}

fn validate_word_source(job: &Job, include_extras: bool) -> Result<(), String> {
    fn check(text: &str) -> Result<(), String> {
        if let Some(ch) = text.chars().find(|ch| !matches!(ch,
            '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')) {
            return Err(format!("Ce document contient un caractère non représentable dans Word (U+{:04X}). Exportez en TXT ou JSON pour conserver la source sans modification.", ch as u32));
        }
        Ok(())
    }
    check(&job.media_name)?;
    for segment in &job.segments {
        check(&segment.text)?;
        if let Some(id) = segment.speaker_id.as_ref() {
            check(job.speaker_names.get(id).map(String::as_str).unwrap_or(id))?;
        }
        if include_extras {
            if let Some(text) = segment.translated_text.as_ref() {
                check(text)?;
            }
        }
    }
    if include_extras {
        if let Some(report) = job.report.as_ref() {
            check(report)?;
        }
    }
    Ok(())
}

pub fn render_docx(job: &Job) -> Result<Vec<u8>, String> {
    validate_word_source(job, true)?;
    let mut body = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>");
    if job.result_incomplete() {
        paragraph(&mut body, INTERRUPTED_EXPORT_TITLE, true);
        paragraph(&mut body, INTERRUPTED_EXPORT_WARNING, false);
    }
    paragraph(&mut body, &job.media_name, true);
    paragraph(&mut body, "Transcription originale", true);
    for s in &job.segments {
        let who = s
            .speaker_id
            .as_deref()
            .map(|id| job.speaker_names.get(id).map(String::as_str).unwrap_or(id))
            .unwrap_or("Locuteur non attribué");
        paragraph(
            &mut body,
            &format!(
                "[{}] {} : {}",
                crate::format_timestamp(s.start_ms),
                who,
                s.text
            ),
            false,
        );
    }
    if job.target_language.is_some() {
        paragraph(&mut body, "Traduction", true);
        for s in &job.segments {
            paragraph(
                &mut body,
                &format!(
                    "[{}] {}",
                    crate::format_timestamp(s.start_ms),
                    s.translated_text
                        .as_deref()
                        .unwrap_or("[traduction manquante]")
                ),
                false,
            );
        }
    }
    if let Some(report) = &job.report {
        paragraph(&mut body, "Compte rendu", true);
        for line in report.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if job.report_format_version == 1 {
                // Classer la structure avant de décoder les champs textuels : un
                // faux titre présent dans une parole ne devient pas un titre Word.
                let text = line.trim_start_matches('#').trim_start_matches('>').trim();
                paragraph(
                    &mut body,
                    &crate::verified_report::decode_encoded_report_text(text),
                    line.starts_with('#'),
                );
            } else {
                // Les anciens rapports n'identifient pas les lignes fiables :
                // conserver leur texte sans promouvoir un titre injecté.
                paragraph(&mut body, line, false);
            }
        }
    }
    body.push_str("<w:sectPr/></w:body></w:document>");
    archive_docx(&body)
}

/// Repère coloré encadré, nom contrasté et paroles aérées : une couleur très
/// claire ne rend jamais le texte illisible ni le repère indiscernable.
fn colored_paragraph(
    out: &mut String,
    speaker: &str,
    timestamp: &str,
    text: &str,
    color: Option<&str>,
) {
    out.push_str("<w:p><w:pPr><w:spacing w:after=\"180\" w:line=\"288\" w:lineRule=\"auto\"/><w:widowControl/></w:pPr>");
    if let Some(color) = color {
        out.push_str("<w:r><w:rPr><w:shd w:val=\"clear\" w:fill=\"");
        out.push_str(&color[1..].to_ascii_uppercase());
        out.push_str("\"/><w:bdr w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"89968F\"/></w:rPr><w:t xml:space=\"preserve\">  </w:t></w:r><w:r><w:t xml:space=\"preserve\">  </w:t></w:r>");
    }
    out.push_str(
        "<w:r><w:rPr><w:b/><w:color w:val=\"223C35\"/></w:rPr><w:t xml:space=\"preserve\">",
    );
    out.push_str(&xml(speaker));
    out.push_str("</w:t></w:r>");
    if !timestamp.is_empty() {
        out.push_str("<w:r><w:rPr><w:color w:val=\"596B63\"/><w:sz w:val=\"19\"/></w:rPr><w:t xml:space=\"preserve\">   ");
        out.push_str(&xml(timestamp.trim()));
        out.push_str("</w:t></w:r>");
    }
    out.push_str("<w:r><w:br/><w:t xml:space=\"preserve\">");
    out.push_str(&xml(text));
    out.push_str("</w:t></w:r></w:p>");
}

pub fn render_presented_docx(
    job: &Job,
    preferences: &PresentationPreferences,
) -> Result<Vec<u8>, String> {
    let blocks = presentation::export_blocks(job, preferences)?;
    validate_word_source(job, preferences.export.content == ExportContent::Complete)?;
    let mut body = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>");
    if job.result_incomplete() {
        paragraph(&mut body, INTERRUPTED_EXPORT_TITLE, true);
        paragraph(&mut body, INTERRUPTED_EXPORT_WARNING, false);
    }
    paragraph(&mut body, &job.media_name, true);
    paragraph(&mut body, "Transcription originale", true);
    let sections =
        if preferences.export.content == ExportContent::Complete && job.target_language.is_some() {
            2
        } else {
            1
        };
    for section in 0..sections {
        if section == 1 {
            paragraph(&mut body, "Traduction", true);
        }
        for block in &blocks {
            let timestamp = if presentation::show_export_timestamps(preferences) {
                format!("[{}] ", crate::format_timestamp(block.start_ms))
            } else {
                String::new()
            };
            let text = presentation::block_text(job, block, section == 1);
            let color = block
                .speaker_id
                .as_ref()
                .and_then(|id| preferences.speaker_colors.get(id))
                .map(String::as_str);
            colored_paragraph(
                &mut body,
                presentation::block_speaker(job, block),
                &timestamp,
                &text,
                color,
            );
        }
    }
    if preferences.export.content == ExportContent::Complete {
        if let Some(report) = &job.report {
            paragraph(&mut body, "Compte rendu", true);
            for line in report.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if job.report_format_version == 1 {
                    let text = line.trim_start_matches('#').trim_start_matches('>').trim();
                    paragraph(
                        &mut body,
                        &crate::verified_report::decode_encoded_report_text(text),
                        line.starts_with('#'),
                    );
                } else {
                    paragraph(&mut body, line, false);
                }
            }
        }
    }
    body.push_str("<w:sectPr/></w:body></w:document>");
    archive_docx(&body)
}

fn archive_docx(body: &str) -> Result<Vec<u8>, String> {
    let content_types = r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
    let document_rels = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let styles = r#"<?xml version="1.0" encoding="UTF-8"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/><w:color w:val="202D28"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="120" w:line="288" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:pPr><w:keepNext/><w:spacing w:before="280" w:after="140"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="28"/><w:color w:val="223C35"/></w:rPr></w:style></w:styles>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, data) in [
        ("[Content_Types].xml", content_types),
        ("_rels/.rels", rels),
        ("word/document.xml", body),
        ("word/styles.xml", styles),
        ("word/_rels/document.xml.rels", document_rels),
    ] {
        archive
            .start_file(name, options)
            .map_err(|e| format!("Export Word impossible : {e}"))?;
        archive
            .write_all(data.as_bytes())
            .map_err(|e| format!("Écriture Word impossible : {e}"))?;
    }
    Ok(archive
        .finish()
        .map_err(|e| format!("Export Word incomplet : {e}"))?
        .into_inner())
}
