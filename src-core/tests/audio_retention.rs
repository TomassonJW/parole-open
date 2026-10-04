#![cfg(unix)]
//! Conservation du fichier exact donné au moteur, sans inférence de modèle.
use parole_core::{
    native::{transcribe_media, NativeTools},
    retained_audio::verify_chunk,
    Job, Stage,
};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

fn wav(seconds: u32) -> Vec<u8> {
    let frames = 16_000 * seconds;
    let data_bytes = frames * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&16_000_u32.to_le_bytes());
    bytes.extend_from_slice(&32_000_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    for i in 0..frames {
        let gain = if (i / 16_000) % 2 == 0 { 100 } else { 200 };
        bytes.extend_from_slice(&(((i % 80) as i16 - 40) * gain).to_le_bytes());
    }
    bytes
}

fn executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn tools(root: &Path, ffmpeg: Option<&Path>, fail_whisper: bool) -> NativeTools {
    fs::write(root.join("decoded.wav"), wav(1)).unwrap();
    let decoder = root.join("decoder.sh");
    executable(&decoder, "#!/bin/sh\nset -eu\nfor output in \"$@\"; do :; done\ncp \"${0%/*}/decoded.wav\" \"$output\"\n");
    let whisper = root.join("whisper-spy.sh");
    let ending = if fail_whisper {
        "exit 7\n"
    } else {
        "printf '%s' '{\"transcription\":[{\"offsets\":{\"from\":0,\"to\":200},\"text\":\"Extrait de test.\"}]}' > \"$output.json\"\n"
    };
    executable(&whisper, &format!("#!/bin/sh\nset -eu\ninput=''\noutput=''\nwhile [ \"$#\" -gt 0 ]; do\n case \"$1\" in\n -f) shift; input=\"$1\";;\n -of) shift; output=\"$1\";;\n esac\n shift\ndone\ncp \"$input\" \"$input.seen-by-whisper\"\nprintf '%s\\n' \"${{input##*/}}\" >> \"${{0%/*}}/whisper.calls\"\n{ending}"));
    let model = root.join("fixture-model.bin");
    fs::write(&model, b"Modele fictif : aucun moteur IA execute").unwrap();
    NativeTools {
        ffmpeg: ffmpeg.unwrap_or(&decoder).to_path_buf(),
        ffprobe: root.join("not-used"),
        whisper,
        model,
    }
}

#[test]
fn conserve_exactement_le_wav_lu_par_whisper_apres_persistance() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let media = root.join("source.wav");
    fs::write(&media, wav(1)).unwrap();
    let tools = tools(root, None, false);
    let state = root.join("travail.json");
    let mut job = Job::new("source.wav".into(), 1_000, 1_000);
    transcribe_media(&mut job, &state, &media, root, &tools, |_| Ok(())).unwrap();
    let saved: Job = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    assert_eq!(saved.stage, Stage::Transcribed);
    assert_eq!(saved.completed_chunks, 1);
    assert_eq!(saved.segments[0].text, "Extrait de test.");
    let retained = root.join("tranche-00000000.wav");
    assert!(
        retained.is_file(),
        "L'extrait relu doit survivre à la transcription"
    );
    assert_eq!(
        fs::read(&retained).unwrap(),
        fs::read(root.join("tranche-00000000.wav.seen-by-whisper")).unwrap()
    );
    assert!(!root.join("tranche-00000000.json").exists());
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("tranche-00000000.audio.json"))
            .expect("La réécoute exige une preuve liée aux octets effectivement transcrits"),
    )
    .unwrap();
    assert_eq!(receipt["schema_version"], 1);
    assert_eq!(receipt["chunk_index"], 0);
    assert_eq!(receipt["timeline_offset_ms"], 0);
    assert_eq!(receipt["duration_ms"], 1_000);
    assert_eq!(receipt["first_segment"], 0);
    assert_eq!(receipt["segment_count"], 1);
    assert_eq!(
        receipt["audio_sha256"],
        parole_core::language::sha256_hex(&fs::read(&retained).unwrap())
    );
    assert_eq!(
        receipt
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "schema_version",
            "chunk_index",
            "timeline_offset_ms",
            "requested_duration_ms",
            "duration_ms",
            "audio_bytes",
            "audio_sha256",
            "job_binding",
            "first_segment",
            "segment_count",
            "transcript_sha256"
        ]
        .into_iter()
        .collect()
    );
    let verified = verify_chunk(
        &saved,
        root.file_name().unwrap().to_str().unwrap(),
        0,
        &serde_json::to_vec(&receipt).unwrap(),
        fs::read(&retained).unwrap(),
    )
    .unwrap();
    assert_eq!(verified.timeline_offset_ms(), 0);
    assert_eq!(verified.duration_ms(), 1_000);
    assert_eq!(
        verified.bytes(),
        fs::read(root.join("tranche-00000000.wav.seen-by-whisper")).unwrap()
    );
}

#[test]
fn une_reprise_conserve_les_extraits_valides_sans_les_retranscrire() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let media = root.join("source.wav");
    fs::write(&media, wav(2)).unwrap();
    let tools = tools(root, None, false);
    let state = root.join("travail.json");
    let mut job = Job::new("source.wav".into(), 2_000, 1_000);
    let interrupted = transcribe_media(&mut job, &state, &media, root, &tools, |_| {
        Err("Pause de test".into())
    });
    assert!(interrupted.is_err());
    let retained = root.join("tranche-00000000.wav");
    assert!(retained.is_file());
    let first = fs::read(&retained).unwrap();
    let first_receipt = fs::read(root.join("tranche-00000000.audio.json")).unwrap();
    let mut resumed: Job = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    assert_eq!(resumed.completed_chunks, 1);
    assert_eq!(resumed.stage, Stage::Interrupted);
    transcribe_media(&mut resumed, &state, &media, root, &tools, |_| Ok(())).unwrap();
    assert_eq!(resumed.completed_chunks, 2);
    assert_eq!(resumed.segments[1].start_ms, 1_000);
    assert_eq!(fs::read(&retained).unwrap(), first);
    assert_eq!(
        fs::read(root.join("tranche-00000000.audio.json")).unwrap(),
        first_receipt
    );
    let second_receipt = fs::read(root.join("tranche-00000001.audio.json")).unwrap();
    let verified = verify_chunk(
        &resumed,
        root.file_name().unwrap().to_str().unwrap(),
        1,
        &second_receipt,
        fs::read(root.join("tranche-00000001.wav")).unwrap(),
    )
    .unwrap();
    assert_eq!(verified.timeline_offset_ms(), 1_000);
    assert_eq!(verified.duration_ms(), 1_000);
    assert_eq!(
        fs::read(root.join("tranche-00000001.wav")).unwrap(),
        fs::read(root.join("tranche-00000001.wav.seen-by-whisper")).unwrap()
    );
    assert_eq!(
        fs::read_to_string(root.join("whisper.calls")).unwrap(),
        "tranche-00000000.wav\ntranche-00000001.wav\n"
    );
}

#[test]
fn un_echec_garde_son_extrait_mais_ne_valide_pas_la_tranche() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let media = root.join("source.wav");
    fs::write(&media, wav(1)).unwrap();
    let tools = tools(root, None, true);
    let state = root.join("travail.json");
    let mut job = Job::new("source.wav".into(), 1_000, 1_000);
    assert!(transcribe_media(&mut job, &state, &media, root, &tools, |_| Ok(())).is_err());
    let saved: Job = serde_json::from_slice(&fs::read(state).unwrap()).unwrap();
    assert_eq!(saved.completed_chunks, 0);
    assert_eq!(saved.stage, Stage::Interrupted);
    assert!(saved.segments.is_empty());
    assert!(root.join("tranche-00000000.wav").is_file());
    assert!(!root.join("tranche-00000000.audio.json").exists());
}

#[test]
fn un_extrait_trop_court_ne_confirme_pas_la_duree_demandee() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let media = root.join("source.wav");
    fs::write(&media, wav(1)).unwrap();
    let tools = tools(root, None, false);
    let state = root.join("travail.json");
    // Le travail attend 30 secondes, mais le décodeur n'en fournit plus qu'une.
    let mut job = Job::new("source.wav".into(), 30_000, 30_000);
    assert!(transcribe_media(&mut job, &state, &media, root, &tools, |_| Ok(())).is_err());
    let saved: Job = serde_json::from_slice(&fs::read(state).unwrap()).unwrap();
    assert_eq!(saved.completed_chunks, 0);
    assert!(saved.segments.is_empty());
    assert!(!root.join("whisper.calls").exists());
    assert!(!root.join("tranche-00000000.audio.json").exists());
}

#[test]
fn un_decodeur_qui_retourne_un_faux_wav_ne_lance_pas_whisper() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let media = root.join("source.wav");
    fs::write(&media, wav(1)).unwrap();
    let tools = tools(root, None, false);
    fs::write(root.join("decoded.wav"), b"ceci n'est pas un WAV").unwrap();
    let state = root.join("travail.json");
    let mut job = Job::new("source.wav".into(), 1_000, 1_000);
    assert!(transcribe_media(&mut job, &state, &media, root, &tools, |_| Ok(())).is_err());
    let saved: Job = serde_json::from_slice(&fs::read(state).unwrap()).unwrap();
    assert_eq!(saved.completed_chunks, 0);
    assert!(saved.segments.is_empty());
    assert!(!root.join("whisper.calls").exists());
    assert!(!root.join("tranche-00000000.audio.json").exists());
}

#[test]
fn une_modification_pendant_whisper_ne_confirme_pas_la_tranche() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let media = root.join("source.wav");
    fs::write(&media, wav(1)).unwrap();
    let tools = tools(root, None, false);
    let spy = fs::read_to_string(&tools.whisper).unwrap();
    executable(
        &tools.whisper,
        &format!("{spy}\nprintf 'x' >> \"$input\"\n"),
    );
    let state = root.join("travail.json");
    let mut job = Job::new("source.wav".into(), 1_000, 1_000);
    assert!(transcribe_media(&mut job, &state, &media, root, &tools, |_| Ok(())).is_err());
    let saved: Job = serde_json::from_slice(&fs::read(state).unwrap()).unwrap();
    assert_eq!(saved.completed_chunks, 0);
    assert!(saved.segments.is_empty());
    assert!(root.join("whisper.calls").is_file());
    assert!(root.join("tranche-00000000.json").is_file());
    assert!(!root.join("tranche-00000000.audio.json").exists());
}

#[test]
fn un_recu_impossible_a_publier_ne_confirme_pas_la_tranche() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let media = root.join("source.wav");
    fs::write(&media, wav(1)).unwrap();
    let tools = tools(root, None, false);
    let destination = root.join("tranche-00000000.audio.json");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("sentinelle"), b"conserver").unwrap();
    let state = root.join("travail.json");
    let mut job = Job::new("source.wav".into(), 1_000, 1_000);
    assert!(transcribe_media(&mut job, &state, &media, root, &tools, |_| Ok(())).is_err());
    let saved: Job = serde_json::from_slice(&fs::read(state).unwrap()).unwrap();
    assert_eq!(saved.completed_chunks, 0);
    assert!(saved.segments.is_empty());
    assert_eq!(
        fs::read(destination.join("sentinelle")).unwrap(),
        b"conserver"
    );
    assert!(root.join("tranche-00000000.json").is_file());
}

#[test]
#[ignore = "Décodage réel : exige PAROLE_TEST_FFMPEG explicite ; Whisper reste simulé"]
fn decodage_reel_conserve_les_deux_fichiers_effectivement_lus() {
    let ffmpeg = std::env::var_os("PAROLE_TEST_FFMPEG")
        .expect("Choisir explicitement un FFmpeg local vérifié");
    let ffmpeg = Path::new(&ffmpeg);
    assert!(ffmpeg.is_absolute() && ffmpeg.is_file());
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let media = root.join("source.wav");
    fs::write(&media, wav(2)).unwrap();
    let tools = tools(root, Some(ffmpeg), false);
    let state = root.join("travail.json");
    let mut job = Job::new("source.wav".into(), 2_000, 1_000);
    transcribe_media(&mut job, &state, &media, root, &tools, |_| Ok(())).unwrap();
    let saved: Job = serde_json::from_slice(&fs::read(state).unwrap()).unwrap();
    assert_eq!(saved.completed_chunks, 2);
    assert!(
        fs::read(root.join("tranche-00000000.wav")).unwrap()
            != fs::read(root.join("tranche-00000001.wav")).unwrap(),
        "Les deux secondes ont des signaux différents : le seek réel doit être respecté"
    );
    for index in 0..2 {
        let stem = format!("tranche-{index:08}.wav");
        let retained = fs::read(root.join(&stem)).unwrap();
        assert_eq!(&retained[..4], b"RIFF");
        assert!(retained.len() >= 32_044 && retained.len() < 33_000);
        let receipt = fs::read(root.join(format!("tranche-{index:08}.audio.json"))).unwrap();
        let verified = verify_chunk(
            &saved,
            root.file_name().unwrap().to_str().unwrap(),
            index,
            &receipt,
            retained.clone(),
        )
        .unwrap();
        assert_eq!(verified.timeline_offset_ms(), index as u64 * 1_000);
        assert_eq!(verified.duration_ms(), 1_000);
        assert_eq!(verified.into_bytes(), retained);
        assert_eq!(
            retained,
            fs::read(root.join(format!("{stem}.seen-by-whisper"))).unwrap()
        );
    }
}
