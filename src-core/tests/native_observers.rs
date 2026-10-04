use parole_core::{
    diarization::DiarizationConfig,
    native::{transcribe_media_diarized_with_steps, NativeTools, TranscriptionCallbacks},
    Job,
};
use std::fs;
#[test]
fn conserve_la_phase_avant_erreur_sans_progression_ni_lancement_de_moteur() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let model = root.join("modele-fictif.bin");
    fs::write(&model, b"Fictif : aucun moteur execute").unwrap();
    let absent = root.join("absent");
    let tools = NativeTools {
        ffmpeg: absent.clone(),
        ffprobe: absent.clone(),
        whisper: absent.clone(),
        model,
    };
    let config = DiarizationConfig::new(absent.clone(), absent.clone(), absent.clone());
    let mut job = Job::new("fictif.wav".into(), 1000, 1000);
    let state = root.join("travail.json");
    let mut steps = Vec::new();
    let mut progress = Vec::new();
    let result = transcribe_media_diarized_with_steps(
        &mut job,
        &state,
        &absent,
        root,
        &tools,
        config,
        TranscriptionCallbacks {
            on_progress: |snapshot: &Job| {
                progress.push(snapshot.completed_chunks);
                Ok(())
            },
            on_step: |index, phase| steps.push((index, phase)),
        },
    );
    assert!(result.is_err());
    assert_eq!(steps, vec![(0, "préparation des modèles de voix")]);
    assert!(progress.is_empty());
    assert_eq!(job.completed_chunks, 0);
    assert!(!state.exists());
}
