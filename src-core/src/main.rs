use parole_core::{
    native::{probe_duration_ms, transcribe_media, NativeTools},
    Job,
};
use std::{env, fs, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().collect();
    if args.len() != 6 && args.len() != 7 {
        eprintln!("Usage : parole-core <média> <dossier-de-travail> <modèle> <whisper-cli> <ffmpeg> [langue-source]");
        std::process::exit(2);
    }
    let media = PathBuf::from(&args[1]);
    let workspace = PathBuf::from(&args[2]);
    let model = PathBuf::from(&args[3]);
    let whisper = PathBuf::from(&args[4]);
    let ffmpeg = PathBuf::from(&args[5]);
    let tools = NativeTools {
        ffprobe: PathBuf::from("ffprobe"),
        ffmpeg,
        whisper,
        model,
    };
    let state = workspace.join("travail.json");
    let mut job: Job = if state.is_file() {
        serde_json::from_slice(&fs::read(&state)?)?
    } else {
        Job::new(
            media
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            probe_duration_ms(&media, &tools.ffprobe)?,
            30_000,
        )
    };
    if let Some(language) = args.get(6) {
        job.source_language = language.to_string_lossy().into_owned();
    }
    transcribe_media(&mut job, &state, &media, &workspace, &tools, |j| {
        eprintln!("Transcription : {:.0}%", j.progress() * 100.0);
        Ok(())
    })?;
    fs::write(
        workspace.join("transcription.txt"),
        parole_core::render_txt(&job),
    )?;
    println!("Transcription conservée dans {}", workspace.display());
    Ok(())
}
