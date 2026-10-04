use parole_core::{docx::render_presented_docx, transcript_presentation::*, Job, Segment, Stage};
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
};

fn segment(start: u64, end: u64, speaker: Option<&str>, text: &str) -> Segment {
    let mut s = Segment::new(start, end, text.into());
    s.speaker_id = speaker.map(str::to_owned);
    s
}
fn job() -> Job {
    let mut j = Job::new("Fiction <réunion>.wav".into(), 30_000, 30_000);
    j.stage = Stage::Transcribed;
    j.segments = vec![
        segment(0, 1000, Some("a"), "Bonjour"),
        segment(3000, 4000, Some("a"), " & suite"),
        segment(6001, 7000, Some("a"), "Après"),
        segment(7000, 8000, Some("b"), "Réponse"),
    ];
    j.speaker_names.insert("a".into(), "Alice".into());
    j.speaker_names.insert("b".into(), "Bob".into());
    j
}
fn partitions(blocks: &[TranscriptBlock]) -> Vec<Vec<usize>> {
    blocks.iter().map(|b| b.segment_indices.clone()).collect()
}
#[test]
fn fixture_is_rust_produced_and_stable() {
    let mut j = job();
    j.timing.platform = "fixture".into();
    j.segments[0].translated_text = Some("Hello".into());
    j.target_language = Some("en".into());
    let p = PresentationPreferences::default();
    let mut encoded_job = serde_json::to_value(&j).unwrap();
    encoded_job
        .as_object_mut()
        .unwrap()
        .insert("id".into(), json!("fiction-01"));
    let fixture = serde_json::to_string_pretty(&json!({"job":encoded_job,"preferences":p,"projection":project("fiction-01",&j,&p.screen).unwrap()})).unwrap() + "\n";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ui/tests/fixtures/transcriptPresentation.generated.json");
    if std::env::var_os("PAROLE_REGENERATE_PRESENTATION_FIXTURE").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &fixture).unwrap();
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), fixture);
}
#[test]
fn defaults_versions_and_strict_json() {
    let p = PresentationPreferences::default();
    assert_eq!(
        serde_json::to_value(&p).unwrap(),
        json!({"schema_version":1,"screen":{"mode":"fluid","pause_ms":2000,"show_timestamps":true},"export":{"linked":true,"view":{"mode":"fluid","pause_ms":2000,"show_timestamps":true},"content":"complete"},"speaker_colors":{}})
    );
    assert!(validate_preferences(&p).is_ok());
    for v in [
        json!({"mode":"fluid","pause_ms":true,"show_timestamps":true}),
        json!({"mode":"detailed","pause_ms":1.0,"show_timestamps":true}),
        json!({"mode":"fluid","pause_ms":2000,"show_timestamps":"yes"}),
        json!({"mode":"fluid","pause_ms":2000,"show_timestamps":true,"extra":0}),
    ] {
        assert!(serde_json::from_value::<ViewOptions>(v).is_err());
    }
    for pause in [1, 499, 10001] {
        let mut v = p.screen.clone();
        v.pause_ms = pause;
        assert!(validate_view(&v).is_err());
    }
    for bad in ["#fff", "#GG0033", "<w:x/>", "#abcdef00"] {
        let mut p = p.clone();
        p.speaker_colors.insert("a".into(), bad.into());
        assert!(validate_preferences(&p).is_err());
    }
    let mut p = p.clone();
    p.schema_version = 2;
    assert!(validate_preferences(&p).is_err());
    assert!(serde_json::from_value::<PresentationPreferences>(
        json!({"schema_version":2,"screen":{},"export":{},"speaker_colors":{}})
    )
    .is_err());
}
#[test]
fn partition_boundaries_and_immutable_source() {
    let j = job();
    let before = serde_json::to_vec(&j.segments).unwrap();
    let v = ViewOptions::default();
    let b = group_segments(&j.segments, &v).unwrap();
    assert_eq!(partitions(&b), vec![vec![0, 1], vec![2], vec![3]]);
    assert_eq!(
        (b[0].start_ms, b[0].end_ms, b[0].speaker_id.as_deref()),
        (0, 4000, Some("a"))
    );
    assert_eq!(serde_json::to_vec(&j.segments).unwrap(), before);
    let mut v = v;
    v.mode = ViewMode::Detailed;
    assert_eq!(
        partitions(&group_segments(&j.segments, &v).unwrap()),
        vec![vec![0], vec![1], vec![2], vec![3]]
    );
    v.mode = ViewMode::Fluid;
    v.pause_ms = 0;
    assert_eq!(
        partitions(&group_segments(&j.segments, &v).unwrap()),
        vec![vec![0], vec![1], vec![2], vec![3]]
    );
}
#[test]
fn unknown_empty_overlap_backward_and_aba_do_not_merge() {
    let ss = vec![
        segment(0, 10, Some("a"), "1"),
        segment(10, 20, None, "2"),
        segment(20, 30, None, "3"),
        segment(30, 40, Some(""), "4"),
        segment(40, 50, Some("a"), "5"),
        segment(50, 60, Some("b"), "6"),
        segment(60, 70, Some("a"), "7"),
        segment(65, 80, Some("a"), "8"),
        segment(90, 85, Some("a"), "9"),
        segment(85, 95, Some("a"), "10"),
        segment(100, 110, Some("a"), "11"),
    ];
    let b = group_segments(&ss, &ViewOptions::default()).unwrap();
    assert_eq!(
        partitions(&b),
        vec![
            vec![0],
            vec![1],
            vec![2],
            vec![3],
            vec![4],
            vec![5],
            vec![6],
            vec![7],
            vec![8],
            vec![9, 10]
        ]
    );
    assert_eq!(b[8].start_ms, 90);
    assert_eq!(b[8].end_ms, 85);
}
#[test]
fn threshold_exact_and_over_with_linked_export() {
    let mut j = job();
    j.segments = vec![
        segment(0, 1000, Some("a"), "One"),
        segment(3000, 4000, Some("a"), "Two"),
        segment(6001, 7000, Some("a"), "Three"),
    ];
    let mut p = PresentationPreferences::default();
    assert_eq!(
        partitions(&group_segments(&j.segments, &p.screen).unwrap()),
        vec![vec![0, 1], vec![2]]
    );
    p.export.view.mode = ViewMode::Detailed;
    assert!(render_presented_txt(&j, &p).unwrap().contains("One Two"));
    p.export.linked = false;
    assert!(!render_presented_txt(&j, &p).unwrap().contains("One Two"));
    p.export.view.mode = ViewMode::Fluid;
    p.export.view.pause_ms = 0;
    assert!(!render_presented_txt(&j, &p).unwrap().contains("One Two"));
    p.export.view.pause_ms = 10_000;
    assert!(render_presented_txt(&j, &p)
        .unwrap()
        .contains("One Two Three"));
}
#[test]
fn complete_reports_keep_historical_rendering() {
    let mut j = job();
    j.segments = vec![segment(
        0,
        100,
        Some("a"),
        "<p> & links [x](https://x.test)",
    )];
    j.report = Some("# Titre\n- preuve [citation](https://example.test)".into());
    j.report_format_version = 1;
    let mut p = PresentationPreferences::default();
    p.screen.mode = ViewMode::Detailed;
    assert_eq!(
        render_presented_txt(&j, &p).unwrap(),
        parole_core::render_complete_txt(&j)
    );
    let markdown = render_presented_markdown(&j, &p).unwrap();
    assert_eq!(
        markdown.split("# Compte rendu").nth(1),
        parole_core::render_complete_markdown(&j)
            .split("# Compte rendu")
            .nth(1)
    );
    let bytes = render_presented_docx(&j, &p).unwrap();
    let mut xml = String::new();
    zip::ZipArchive::new(Cursor::new(bytes))
        .unwrap()
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("&lt;p&gt; &amp; links"));
    assert!(!xml.contains("<p>"));
    assert!(xml.contains("Titre"));
}
#[test]
fn markdown_keeps_distinct_paragraphs_and_exports_real_documents() {
    let mut j = job();
    j.media_name = "Atelier de démonstration".into();
    j.segments = vec![
        segment(
            0,
            2100,
            Some("a"),
            "Bonjour à tous. Voici une transcription inventée pour vérifier la lecture.",
        ),
        segment(
            2800,
            5500,
            Some("a"),
            "Les petites pauses ne répètent plus le nom ni l’horaire.",
        ),
        segment(
            9000,
            12500,
            Some("a"),
            "Après une pause plus longue, une nouvelle prise de parole commence.",
        ),
        segment(
            13000,
            16000,
            Some("b"),
            "Chaque personne peut choisir sa couleur, sans modifier les mots.",
        ),
        segment(
            16800,
            19000,
            Some("b"),
            "Même une couleur claire doit laisser le texte lisible.",
        ),
        segment(
            22000,
            25000,
            None,
            "Ce passage reste sans locuteur attribué.",
        ),
    ];
    j.segments[0].translated_text = Some("Hello everyone. This is a fictional transcript.".into());
    j.target_language = Some("en".into());
    j.report = Some("# Note de démonstration\n\nAucune réunion réelle n’est utilisée.".into());
    j.report_format_version = 1;
    let mut p = PresentationPreferences {
        speaker_colors: BTreeMap::from([
            ("a".into(), "#276B64".into()),
            ("b".into(), "#FFFFFF".into()),
        ]),
        ..PresentationPreferences::default()
    };
    let markdown = render_presented_markdown(&j, &p).unwrap();
    assert!(
        markdown.contains("l’horaire&#46;\n\n[00:00:09,000] **Alice**"),
        "Chaque prise de parole doit former un vrai paragraphe Markdown"
    );
    assert!(markdown.contains("commence&#46;\n\n[00:00:13,000] **Bob**"));
    if let Ok(directory) = std::env::var("PAROLE_PRESENTATION_EVIDENCE_DIR") {
        let directory = std::path::Path::new(&directory);
        std::fs::create_dir_all(directory).unwrap();
        std::fs::write(directory.join("lecture-fluide.md"), markdown).unwrap();
        std::fs::write(
            directory.join("lecture-fluide.txt"),
            render_presented_txt(&j, &p).unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.join("lecture-fluide.docx"),
            render_presented_docx(&j, &p).unwrap(),
        )
        .unwrap();
        p.export.linked = false;
        p.export.view.show_timestamps = false;
        p.export.content = ExportContent::Transcript;
        std::fs::write(
            directory.join("sans-horaires.docx"),
            render_presented_docx(&j, &p).unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.join("sans-horaires.txt"),
            render_presented_txt(&j, &p).unwrap(),
        )
        .unwrap();
        let mut projections = Vec::new();
        for mode in [ViewMode::Fluid, ViewMode::Detailed] {
            for pause_ms in (0..=10000).step_by(500) {
                for show_timestamps in [false, true] {
                    projections.push(
                        project(
                            "fiction-visual",
                            &j,
                            &ViewOptions {
                                mode,
                                pause_ms,
                                show_timestamps,
                            },
                        )
                        .unwrap(),
                    );
                }
            }
        }
        let mut encoded_job = serde_json::to_value(&j).unwrap();
        encoded_job
            .as_object_mut()
            .unwrap()
            .insert("id".into(), json!("fiction-visual"));
        std::fs::write(
            directory.join("visual-fixture.json"),
            serde_json::to_vec_pretty(
                &json!({"job":encoded_job,"preferences":p,"projections":projections}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn word_refuse_les_caracteres_interdits_xml_sans_alterer_la_source() {
    for forbidden in ['\u{0000}', '\u{000b}', '\u{fffe}', '\u{ffff}'] {
        let mut j = job();
        j.segments = vec![segment(0, 1000, Some("a"), &format!("A{forbidden}B"))];
        let before = serde_json::to_vec(&j).unwrap();
        let p = PresentationPreferences::default();
        let error = match render_presented_docx(&j, &p) {
            Err(error) => error,
            Ok(_) => panic!("Un fichier Word invalide ne doit jamais être annoncé réussi"),
        };
        assert!(error.contains("TXT") && error.contains("JSON"));
        assert!(parole_core::docx::render_docx(&j).is_err());
        assert!(render_presented_txt(&j, &p)
            .unwrap()
            .contains(&format!("A{forbidden}B")));
        assert_eq!(before, serde_json::to_vec(&j).unwrap());
    }
    for field in ["media", "speaker", "translation", "report"] {
        let mut j = job();
        j.segments = vec![segment(0, 1000, Some("a"), "Paroles conservées")];
        match field {
            "media" => j.media_name = "Média\u{ffff}".into(),
            "speaker" => {
                j.speaker_names.insert("a".into(), "Nom\u{ffff}".into());
            }
            "translation" => j.segments[0].translated_text = Some("Traduction\u{ffff}".into()),
            "report" => j.report = Some("Rapport\u{ffff}".into()),
            _ => unreachable!(),
        }
        let mut p = PresentationPreferences::default();
        assert!(render_presented_docx(&j, &p).is_err(), "champ {field}");
        if field == "translation" || field == "report" {
            p.export.content = ExportContent::Transcript;
            assert!(
                render_presented_docx(&j, &p).is_ok(),
                "un contenu exclu ne doit pas bloquer l’export"
            );
        }
    }
    let mut j = job();
    j.segments = vec![segment(
        0,
        1000,
        Some("a"),
        "Accent é, tabulation\t, saut\n, emoji 🗣 et musical 𝄞",
    )];
    assert!(render_presented_docx(&j, &PresentationPreferences::default()).is_ok());
}

#[test]
fn large_partition_is_linear_and_no_theme_cap() {
    let ss: Vec<_> = (0..100_101_u64)
        .map(|i| segment(i * 10, i * 10 + 5, Some("a"), "x"))
        .collect();
    let b = group_segments(&ss, &ViewOptions::default()).unwrap();
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].segment_indices.len(), 100_101);
}
#[test]
fn revision_exact_tuple_json_and_projection() {
    let j = job();
    let expected = parole_core::language::sha256_hex(
        &serde_json::to_vec(
            &j.segments
                .iter()
                .map(|s| {
                    (
                        &s.start_ms,
                        &s.end_ms,
                        &s.text,
                        &s.speaker_id,
                        &s.translated_text,
                    )
                })
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    );
    assert_eq!(source_revision(&j.segments).unwrap(), expected);
    let snap = project("fiction-id", &j, &ViewOptions::default()).unwrap();
    assert_eq!(snap.schema_version, 1);
    assert_eq!(snap.job_id, "fiction-id");
    assert_eq!(snap.source_revision, expected);
    assert_eq!(partitions(&snap.blocks), vec![vec![0, 1], vec![2], vec![3]]);
}
#[test]
fn exports_complete_transcript_partial_and_colors() {
    let mut j = job();
    j.segments[0].translated_text = Some("Hello".into());
    j.target_language = Some("en".into());
    j.report = Some("## Rapport fictif".into());
    j.report_format_version = 1;
    let mut p = PresentationPreferences {
        speaker_colors: BTreeMap::from([("a".into(), "#aB12ef".into())]),
        ..Default::default()
    };
    p.export.linked = false;
    p.export.view.show_timestamps = false;
    let txt = render_presented_txt(&j, &p).unwrap();
    let md = render_presented_markdown(&j, &p).unwrap();
    assert!(txt.contains("Bonjour  & suite"));
    assert!(md.contains("Bonjour  &amp; suite"));
    assert!(txt.contains("[traduction manquante]"));
    assert!(md.contains("\\[traduction manquante\\]"));
    for content in [&txt, &md] {
        assert!(!content.contains("00:00:00"));
        assert!(content.contains("Rapport fictif"));
        assert!(content.contains("Résultat incomplet"));
    }
    let bytes = render_presented_docx(&j, &p).unwrap();
    let mut xml = String::new();
    zip::ZipArchive::new(Cursor::new(bytes))
        .unwrap()
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("AB12EF"));
    assert!(
        xml.contains("<w:spacing w:after=\"180\""),
        "Les prises de parole Word doivent être espacées"
    );
    assert!(
        xml.contains("<w:b/>"),
        "Le nom doit se distinguer des paroles"
    );
    assert!(
        xml.contains("<w:br/>"),
        "Les paroles commencent sous le nom et son horaire"
    );
    let archive_bytes = render_presented_docx(&j, &p).unwrap();
    let mut package = zip::ZipArchive::new(Cursor::new(archive_bytes)).unwrap();
    let mut styles = String::new();
    package
        .by_name("word/styles.xml")
        .expect("Les styles Word sont déclarés")
        .read_to_string(&mut styles)
        .unwrap();
    assert!(styles.contains("w:styleId=\"Heading1\""));
    assert!(xml.contains("Alice"));
    assert!(xml.contains("Bonjour  &amp; suite"));
    assert!(xml.contains("[traduction manquante]"));
    p.export.content = ExportContent::Transcript;
    for content in [
        render_presented_txt(&j, &p).unwrap(),
        render_presented_markdown(&j, &p).unwrap(),
    ] {
        assert!(content.contains("Résultat incomplet"));
        assert!(!content.contains("Traduction\n"));
        assert!(!content.contains("Rapport fictif"));
    }
    let bytes = render_presented_docx(&j, &p).unwrap();
    let mut xml = String::new();
    zip::ZipArchive::new(Cursor::new(bytes))
        .unwrap()
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(!xml.contains("Rapport fictif"));
    assert!(!xml.contains("[traduction manquante]"));
    p.speaker_colors.insert("absent".into(), "#123456".into());
    assert!(render_presented_docx(&j, &p).is_err());
}
