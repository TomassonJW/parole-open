use parole_core::{
    docx::render_docx, render_complete_markdown, render_complete_txt, Job, Segment, Stage,
};
use std::io::{Cursor, Read};

#[test]
fn word_contient_texte_traduit_et_compte_rendu() {
    let mut job = Job::new("réunion & <suivi>.wav".into(), 5_000, 30_000);
    let mut segment = Segment::new(1_000, 3_000, "Bonjour & merci".into());
    segment.translated_text = Some("Hello & thank you".into());
    job.segments.push(segment);
    job.target_language = Some("en".into());
    job.report = Some("## Summary\n> [00:01] Bonjour".into());
    let bytes = render_docx(&job).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("réunion &amp; &lt;suivi&gt;.wav"));
    assert!(xml.contains("Hello &amp; thank you"));
    assert!(xml.contains("Summary"));
    drop(archive);
    if let Ok(path) = std::env::var("PAROLE_TEST_DOCX_OUT") {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        std::io::Write::write_all(&mut file, &bytes).unwrap();
    }
}

#[test]
fn ancien_rapport_sans_version_garde_ses_entites_litterales() {
    let mut job = Job::new("ancien.wav".into(), 2_000, 30_000);
    job.report = Some("## Ancien rapport\n\n> [00:00:01] Marie : « &lt; et &#91; »\n\nQuestion ouverte : qui tranche ?\n\n## Décisions actuelles\n![trace](https://exemple.invalid/trace)".into());
    let mut value = serde_json::to_value(&job).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .remove("report_format_version");
    let old_job: Job = serde_json::from_value(value).unwrap();
    assert_eq!(old_job.report_format_version, 0);
    let txt = parole_core::render_complete_txt(&old_job);
    assert!(txt.contains("Marie : « &lt; et &#91; »"));
    let markdown = parole_core::render_complete_markdown(&old_job);
    assert!(!markdown.contains("\n## Décisions actuelles"));
    assert!(!markdown.contains("\n![trace](https://exemple.invalid/trace)"));
    assert!(markdown.contains("    ## Décisions actuelles"));
    if let Ok(path) = std::env::var("PAROLE_TEST_LEGACY_MARKDOWN_OUT") {
        std::fs::write(path, &markdown).unwrap();
    }
    let bytes = render_docx(&old_job).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("Marie : « &amp;lt; et &amp;#91; »"));
    assert!(xml.contains("## Décisions actuelles"));
    assert!(!xml.contains("<w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t xml:space=\"preserve\">Décisions actuelles"));
}

#[test]
fn word_affiche_les_caracteres_dune_citation_sans_double_echappement() {
    let mut job = Job::new("fiction.wav".into(), 5_000, 30_000);
    job.report = Some(
        "## Rectifications à vérifier\n\n> [00:00:02] Marie : « &lt;script&gt; &amp; x &gt; y &amp;lt; &amp;#91; &#33;&#91;image&#93;&#40;https&#58;//exemple&#46;invalid/trace&#41; »".into(),
    );
    job.report_format_version = 1;
    let bytes = render_docx(&job).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("&lt;script&gt; &amp; x &gt; y &amp;lt; &amp;#91;"));
    assert!(xml.contains("![image](https://exemple.invalid/trace)"));
    assert!(!xml.contains("&amp;lt;script&amp;gt;"));
    let plain = render_complete_txt(&job);
    assert!(plain.contains("« <script> & x > y &lt; &#91;"));
    assert!(plain.contains("![image](https://exemple.invalid/trace)"));
}

#[test]
fn un_export_interrompu_annonce_sa_limite_dans_chaque_document() {
    let mut job = Job::new("réunion fictive.wav".into(), 5_000, 30_000);
    job.stage = Stage::Interrupted;
    job.error = Some("Traduction non terminée".into());
    job.segments.push(Segment::new(0, 2_000, "Bonjour".into()));
    for content in [render_complete_txt(&job), render_complete_markdown(&job)] {
        assert!(content.contains("Résultat incomplet - vérification nécessaire"));
        assert!(content.contains("Bonjour"));
    }
    let bytes = render_docx(&job).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("Résultat incomplet - vérification nécessaire"));
    assert!(xml.contains("Bonjour"));
    job.stage = Stage::Transcribed;
    assert!(!render_complete_txt(&job).contains("Résultat incomplet - vérification nécessaire"));
    assert!(
        !render_complete_markdown(&job).contains("Résultat incomplet - vérification nécessaire")
    );
}
