use parole_core::{
    diarization::DiarizationConfig,
    language::{
        build_report, translate_job, Language, LlamaCppEngine, ReportOptions, TranslationOptions,
        MODELE_TEXTE_RECOMMANDE,
    },
    native::{probe_duration_ms, transcribe_media_diarized, NativeTools},
    verified_report::render_verified_report,
    Job, Stage,
};
use std::{collections::BTreeSet, path::PathBuf};

#[test]
fn deux_voix_attribuees_sur_un_vrai_enregistrement() {
    let Ok(root) = std::env::var("PAROLE_DIARIZATION_ASSETS") else {
        if std::env::var_os("PAROLE_REQUIRE_DIARIZATION").is_some() {
            panic!("ressources manquantes");
        }
        return;
    };
    let a = PathBuf::from(root);
    let media = a.join("2-two-speakers-en.wav");
    let tools = NativeTools {
        ffmpeg: PathBuf::from("/usr/bin/ffmpeg"),
        ffprobe: PathBuf::from("/usr/bin/ffprobe"),
        whisper: PathBuf::from(std::env::var("PAROLE_TEST_WHISPER").expect("chemin du moteur")),
        model: PathBuf::from(std::env::var("PAROLE_TEST_MODEL").expect("chemin du modèle")),
    };
    let config = DiarizationConfig::new(
        a.join("lib/libsherpa-onnx-c-api.so"),
        a.join("sherpa-onnx-pyannote-segmentation-3-0/model.onnx"),
        a.join("3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx"),
    );
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("travail.json");
    let mut job = Job::new(
        "deux-voix.wav".into(),
        probe_duration_ms(&media, &tools.ffprobe).unwrap(),
        30_000,
    );
    job.source_language = "en".into();
    transcribe_media_diarized(&mut job, &state, &media, dir.path(), &tools, config, |_| {
        Ok(())
    })
    .unwrap();
    assert_eq!(job.stage, Stage::Transcribed);
    let names: BTreeSet<_> = job
        .segments
        .iter()
        .filter_map(|s| s.speaker_id.clone())
        .collect();
    println!("{} locuteurs, {} passages", names.len(), job.segments.len());
    assert!(
        names.len() >= 2,
        "deux voix réelles doivent rester distinctes"
    );
    let (Ok(binary), Ok(model)) = (
        std::env::var("PAROLE_LLAMA_BIN"),
        std::env::var("PAROLE_LLM_MODEL"),
    ) else {
        return;
    };
    let mut engine = LlamaCppEngine::new(binary.into(), model.into(), dir.path().to_path_buf());
    engine.check(&MODELE_TEXTE_RECOMMANDE).unwrap();
    let options = TranslationOptions::new(Language::French);
    let interrupted = translate_job(&mut job, &state, &mut engine, &options, |done, _| {
        if done >= 1 {
            Err("arrêt volontaire".into())
        } else {
            Ok(())
        }
    });
    assert!(interrupted.is_err());
    let saved: Vec<_> = job
        .segments
        .iter()
        .map(|s| s.translated_text.clone())
        .collect();
    assert!(saved.iter().any(Option::is_some));
    assert!(saved.iter().any(Option::is_none));
    drop(engine);
    let mut resumed: Job = serde_json::from_slice(&std::fs::read(&state).unwrap()).unwrap();
    let mut second = LlamaCppEngine::new(
        std::env::var("PAROLE_LLAMA_BIN").unwrap().into(),
        std::env::var("PAROLE_LLM_MODEL").unwrap().into(),
        dir.path().to_path_buf(),
    );
    translate_job(&mut resumed, &state, &mut second, &options, |_, _| Ok(())).unwrap();
    assert!(resumed
        .segments
        .iter()
        .all(|s| s.translated_text.as_ref().is_some_and(|t| !t.is_empty())));
    assert_eq!(
        resumed
            .segments
            .iter()
            .map(|s| s.speaker_id.clone())
            .collect::<Vec<_>>(),
        job.segments
            .iter()
            .map(|s| s.speaker_id.clone())
            .collect::<Vec<_>>()
    );
    resumed.target_language = Some("fr".into());
    let mut report_options = ReportOptions::new(Language::French);
    report_options.use_translation = true;
    let report = build_report(
        &resumed,
        &dir.path().join("report-state.json"),
        &mut second,
        &report_options,
        |_, _| Ok(()),
    )
    .unwrap();
    let verified = render_verified_report(&resumed, &report);
    assert!(verified.contains("## Synthèse"));
    for (old, segment) in saved.iter().zip(&resumed.segments) {
        if old.is_some() {
            assert_eq!(old, &segment.translated_text);
        }
    }
    assert!(verified.lines().any(|l| l.starts_with("> [")));
    for line in verified.lines().filter(|l| l.starts_with("> [")) {
        assert!(resumed
            .segments
            .iter()
            .any(|s| s.translated_text.as_ref().is_some_and(|t| line.contains(t))));
    }
}
