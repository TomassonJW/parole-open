use super::*;
use crate::{process_chunks, Stage};
use serde_json::{json, Value};

fn verify_fixture(
    job: &Job,
    index: usize,
    receipt: &[u8],
    bytes: Vec<u8>,
) -> Result<VerifiedAudio, String> {
    super::verify_chunk(job, "fixture-A", index, receipt, bytes)
}

fn wave() -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&32_036u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&[1, 0, 1, 0, 128, 62, 0, 0, 0, 125, 0, 0, 2, 0, 16, 0]);
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&32_000u32.to_le_bytes());
    bytes.resize(32_044, 0);
    bytes
}

// Le reçu provient du vrai producteur, puis le Job est réellement relu sur disque.
// Aucun moteur d'inférence n'intervient : paroles fictives et signal nul.
fn fixture() -> (Job, Vec<u8>, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let state = root.join("travail.json");
    let mut job = Job::new("fictif.wav".into(), 2_000, 1_000);
    let binding = job_binding(&job, "fixture-A");
    let mut first_segment = 0;
    process_chunks(&mut job, &state, |index, start, duration| {
        let path = root.join(format!("tranche-{index:08}.wav"));
        fs::write(&path, wave()).unwrap();
        let prepared = prepare(&path, duration)?;
        let segments = vec![Segment {
            start_ms: 100,
            end_ms: 800,
            text: format!("Paroles fictives {index}."),
            speaker_id: Some("voix-1".into()),
            translated_text: None,
        }];
        retain_receipt(
            root,
            index,
            start,
            duration,
            &binding,
            first_segment,
            &segments,
            prepared,
        )?;
        first_segment += segments.len();
        Ok(segments)
    })
    .unwrap();
    let saved = serde_json::from_slice(&fs::read(state).unwrap()).unwrap();
    let receipt = fs::read(root.join("tranche-00000001.audio.json")).unwrap();
    let audio = fs::read(root.join("tranche-00000001.wav")).unwrap();
    (saved, receipt, audio)
}

#[test]
fn deux_travaux_identiques_ne_partagent_pas_leur_identite_audio() {
    let (job, receipt, bytes) = fixture();
    assert!(super::verify_chunk(&job, "fixture-B", 1, &receipt, bytes).is_err());
}

#[test]
fn preuve_persistante_lue_par_le_consommateur_reel() {
    let (job, receipt, bytes) = fixture();
    let verified = verify_fixture(&job, 1, &receipt, bytes.clone()).unwrap();
    assert_eq!(verified.bytes(), bytes);
    assert_eq!(verified.timeline_offset_ms(), 1_000);
    assert_eq!(verified.duration_ms(), 1_000);
    assert_eq!(verified.into_bytes(), bytes);
}

#[test]
fn refuse_octet_modifie_paroles_modifiees_et_horodatage_modifie() {
    let (job, receipt, bytes) = fixture();
    let mut altered = bytes.clone();
    *altered.last_mut().unwrap() = 1;
    assert!(verify_fixture(&job, 1, &receipt, altered).is_err());
    let mut changed = job.clone();
    changed.segments[1].text.push('!');
    assert!(verify_fixture(&changed, 1, &receipt, bytes.clone()).is_err());
    changed = job.clone();
    changed.segments[1].start_ms += 1;
    assert!(verify_fixture(&changed, 1, &receipt, bytes).is_err());
}

#[test]
fn conserve_la_lecture_apres_traduction_renommage_et_deplacement_source() {
    let (mut job, receipt, bytes) = fixture();
    job.target_language = Some("en".into());
    job.segments[1].translated_text = Some("Fictional words.".into());
    job.speaker_names.insert("voix-1".into(), "Camille".into());
    job.source_path = Some("chemin-deplace/fictif.wav".into());
    job.stage = Stage::Transcribed;
    job.report = Some("Compte rendu fictif".into());
    assert!(verify_fixture(&job, 1, &receipt, bytes).is_ok());
}

#[test]
fn refuse_tranche_non_confirmee_et_recu_dun_autre_travail() {
    let (mut job, receipt, bytes) = fixture();
    job.completed_chunks = 1;
    assert!(verify_fixture(&job, 1, &receipt, bytes.clone()).is_err());
    job.completed_chunks = 2;
    job.media_name = "autre.wav".into();
    assert!(verify_fixture(&job, 1, &receipt, bytes.clone()).is_err());
    job.media_name = "fictif.wav".into();
    assert!(verify_fixture(&job, 0, &receipt, bytes).is_err());
}

#[test]
fn refuse_etat_de_reprise_incoherent_et_depassements() {
    let (job, receipt, bytes) = fixture();
    for (chunk_ms, duration_ms, completed_chunks) in [
        (0, 2_000, 2),
        (1_000, 0, 2),
        (1_000, 2_000, 3),
        (1_000, 2_000, 0),
    ] {
        let mut invalid = job.clone();
        invalid.chunk_ms = chunk_ms;
        invalid.duration_ms = duration_ms;
        invalid.completed_chunks = completed_chunks;
        assert!(verify_fixture(&invalid, 1, &receipt, bytes.clone()).is_err());
    }
    assert!(verify_fixture(&job, usize::MAX, &receipt, bytes).is_err());
}

#[test]
fn refuse_recu_absent_inconnu_surdimensionne_ou_malforme() {
    let (job, receipt, bytes) = fixture();
    for invalid in [b"".as_slice(), b"null", b"{}", b"[]", b"{broken"] {
        assert!(verify_fixture(&job, 1, invalid, bytes.clone()).is_err());
    }
    assert!(verify_fixture(&job, 1, &vec![b' '; MAX_RECEIPT_BYTES + 1], bytes.clone()).is_err());
    let value: Value = serde_json::from_slice(&receipt).unwrap();
    for (key, wrong) in [
        ("schema_version", json!(true)),
        ("schema_version", json!(2)),
        ("chunk_index", json!(0)),
        ("timeline_offset_ms", json!(0)),
        ("requested_duration_ms", json!(999)),
        ("duration_ms", json!(999)),
        ("audio_bytes", json!(1)),
        ("first_segment", json!(usize::MAX)),
        ("segment_count", json!(usize::MAX)),
        ("segment_count", json!(0)),
        ("audio_sha256", json!("00")),
        ("transcript_sha256", json!("00")),
        ("job_binding", json!("00")),
        ("source_path", json!("interdit.wav")),
    ] {
        let mut changed = value.clone();
        changed[key] = wrong;
        assert!(
            verify_fixture(
                &job,
                1,
                &serde_json::to_vec(&changed).unwrap(),
                bytes.clone()
            )
            .is_err(),
            "Champ accepté à tort : {key}"
        );
    }
}

#[test]
fn controle_le_format_wav_meme_avec_une_empreinte_coherente() {
    let (job, receipt, bytes) = fixture();
    let original: Value = serde_json::from_slice(&receipt).unwrap();
    // Signature, conteneur, PCM, mono, taux, débit, alignement, résolution, taille.
    for position in [0, 4, 8, 20, 22, 24, 28, 32, 34, 40] {
        let mut changed = bytes.clone();
        changed[position] ^= 1;
        let mut value = original.clone();
        value["audio_sha256"] = json!(sha256_hex(&changed));
        assert!(
            verify_fixture(&job, 1, &serde_json::to_vec(&value).unwrap(), changed).is_err(),
            "Entête accepté à tort : {position}"
        );
    }
}

#[test]
fn refuse_audio_surdimensionne_sans_analyser_un_faux_conteneur() {
    let (job, receipt, _) = fixture();
    assert!(verify_fixture(&job, 1, &receipt, vec![0; MAX_AUDIO_BYTES + 1]).is_err());
}

#[test]
fn analyse_blocs_optionnels_et_refuse_doublons_et_blocs_tronques() {
    let bytes = wave();
    let mut with_junk = bytes[..12].to_vec();
    with_junk.extend_from_slice(b"JUNK\x01\x00\x00\x00x\x00");
    with_junk.extend_from_slice(&bytes[12..]);
    let size = (with_junk.len() - 8) as u32;
    with_junk[4..8].copy_from_slice(&size.to_le_bytes());
    assert_eq!(wav_duration_ms(&with_junk).unwrap(), 1_000);
    for duplicate in [&bytes[12..36], &bytes[36..]] {
        let mut altered = bytes.clone();
        altered.extend_from_slice(duplicate);
        let size = (altered.len() - 8) as u32;
        altered[4..8].copy_from_slice(&size.to_le_bytes());
        assert!(wav_duration_ms(&altered).is_err());
    }
    for length in [0, 11, 43, 44, 100, bytes.len() - 1] {
        let mut altered = bytes[..length].to_vec();
        if length >= 8 {
            altered[4..8].copy_from_slice(&((length - 8) as u32).to_le_bytes());
        }
        assert!(wav_duration_ms(&altered).is_err());
    }
}

#[test]
fn ne_confond_pas_un_recu_existant_avec_une_tranche_confirmee() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("tranche-00000000.wav");
    fs::write(&path, wave()).unwrap();
    let job = Job::new("fictif.wav".into(), 1_000, 1_000);
    retain_receipt(
        root.path(),
        0,
        0,
        1_000,
        &job_binding(&job, "fixture-A"),
        0,
        &[],
        prepare(&path, 1_000).unwrap(),
    )
    .unwrap();
    let receipt = fs::read(root.path().join("tranche-00000000.audio.json")).unwrap();
    assert!(verify_fixture(&job, 0, &receipt, wave()).is_err());
    let mut saved = job;
    saved.completed_chunks = 1;
    // Une tranche réellement silencieuse peut légitimement n'avoir aucun passage.
    assert!(verify_fixture(&saved, 0, &receipt, wave()).is_ok());
}

#[test]
fn conserve_la_preuve_precedente_si_la_nouvelle_ecriture_echoue() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("tranche-00000000.wav");
    fs::write(&path, wave()).unwrap();
    let destination = root.path().join("tranche-00000000.audio.json");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("sentinelle"), b"ne pas effacer").unwrap();
    let prepared = prepare(&path, 1_000).unwrap();
    assert!(retain_receipt(root.path(), 0, 0, 1_000, "test", 0, &[], prepared).is_err());
    assert_eq!(
        fs::read(destination.join("sentinelle")).unwrap(),
        b"ne pas effacer"
    );
    assert_eq!(fs::read(&path).unwrap(), wave());
    assert!(fs::read_dir(root.path()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains(".part-")));
}

#[test]
fn refuse_modification_de_laudio_entre_preparation_et_transcription() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("tranche-00000000.wav");
    let bytes = wave();
    fs::write(&path, &bytes).unwrap();
    let prepared = prepare(&path, 1_000).unwrap();
    let mut changed = bytes;
    *changed.last_mut().unwrap() = 1;
    fs::write(&path, &changed).unwrap();
    assert!(retain_receipt(root.path(), 0, 0, 1_000, "test", 0, &[], prepared).is_err());
    assert!(!root.path().join("tranche-00000000.audio.json").exists());
    assert_eq!(fs::read(&path).unwrap(), changed);
}

#[test]
fn refuse_des_horodatages_apres_la_fin_reelle_meme_dans_la_tolerance_demandee() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("tranche-00000000.wav");
    fs::write(&path, wave()).unwrap();
    let segment = Segment {
        start_ms: 1_000,
        end_ms: 1_100,
        text: "Hors audio".into(),
        speaker_id: None,
        translated_text: None,
    };
    // L'écart de durée de 100 ms est tolérable, pas un passage après la fin du WAV.
    assert!(retain_receipt(
        root.path(),
        0,
        0,
        1_100,
        "test",
        0,
        &[segment],
        prepare(&path, 1_100).unwrap()
    )
    .is_err());
    assert!(!root.path().join("tranche-00000000.audio.json").exists());
}

#[test]
fn refuse_une_ancienne_preuve_daudio_trop_court() {
    let (mut job, receipt, bytes) = fixture();
    let mut value: Value = serde_json::from_slice(&receipt).unwrap();
    // Fixture négative : cohérence des empreintes, mais une seconde au lieu de trente.
    job.duration_ms = 60_000;
    job.chunk_ms = 30_000;
    job.segments[1].start_ms = 30_100;
    job.segments[1].end_ms = 30_800;
    value["requested_duration_ms"] = json!(30_000);
    value["timeline_offset_ms"] = json!(30_000);
    value["job_binding"] = json!(job_binding(&job, "fixture-A"));
    value["transcript_sha256"] = json!(transcript_hash(&job.segments[1..], 0).unwrap());
    assert!(verify_fixture(&job, 1, &serde_json::to_vec(&value).unwrap(), bytes).is_err());
}

#[test]
fn tolerance_bornee_sans_accepter_un_debut_hors_audio() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audio.wav");
    fs::write(&path, wave()).unwrap();
    for requested in [800, 1_000, 1_200] {
        assert!(prepare(&path, requested).is_ok());
    }
    for requested in [799, 1_201, 30_000] {
        assert!(prepare(&path, requested).is_err());
    }
    let mut segment = Segment {
        start_ms: 900,
        end_ms: 1_150,
        text: "Fictif".into(),
        speaker_id: None,
        translated_text: None,
    };
    assert!(validate_times(&[segment.clone()], 0, 1_100, 1_000).is_ok());
    segment.end_ms = 1_201;
    assert!(validate_times(&[segment.clone()], 0, 1_100, 1_000).is_err());
    segment.start_ms = 1_000;
    segment.end_ms = 1_100;
    assert!(validate_times(&[segment], 0, 1_100, 1_000).is_err());
}

#[test]
fn consommateur_refuse_un_debut_hors_audio_malgre_les_empreintes_coherentes() {
    let (mut job, receipt, bytes) = fixture();
    let mut value: Value = serde_json::from_slice(&receipt).unwrap();
    job.segments[1].start_ms = 2_000;
    job.segments[1].end_ms = 2_100;
    value["transcript_sha256"] = json!(transcript_hash(&job.segments[1..], 0).unwrap());
    assert!(verify_fixture(&job, 1, &serde_json::to_vec(&value).unwrap(), bytes).is_err());
}

#[test]
fn refuse_horodatages_invalides_avant_publication_du_recu() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("tranche-00000000.wav");
    fs::write(&path, wave()).unwrap();
    for (start_ms, end_ms, offset) in [(500, 400, 0), (0, 1_201, 0), (0, 800, u64::MAX)] {
        let segments = [Segment {
            start_ms,
            end_ms,
            text: "Fictif".into(),
            speaker_id: None,
            translated_text: None,
        }];
        assert!(retain_receipt(
            root.path(),
            0,
            offset,
            1_000,
            "test",
            0,
            &segments,
            prepare(&path, 1_000).unwrap()
        )
        .is_err());
        assert!(!root.path().join("tranche-00000000.audio.json").exists());
    }
}
