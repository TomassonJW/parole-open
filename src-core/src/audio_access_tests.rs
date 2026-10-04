use super::*;
use crate::{
    process_chunks,
    retained_audio::{job_binding, prepare, retain_receipt},
    Segment,
};
use std::fs;
const ID: &str = "11111111-1111-4111-8111-111111111111";

fn wave(index: usize) -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&32_036u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&[1, 0, 1, 0, 128, 62, 0, 0, 0, 125, 0, 0, 2, 0, 16, 0]);
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&32_000u32.to_le_bytes());
    bytes.resize(32_044, index as u8);
    bytes
}

fn fixture() -> tempfile::TempDir {
    fixture_named(ID)
}
fn fixture_named(id: &str) -> tempfile::TempDir {
    fixture_options(id, 2_000, 1_000)
}
fn fixture_options(id: &str, duration: u64, chunk: u64) -> tempfile::TempDir {
    fixture_wave(id, duration, chunk, wave)
}
fn fixture_wave(
    id: &str,
    duration: u64,
    chunk: u64,
    make_wave: fn(usize) -> Vec<u8>,
) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("jobs").join(id);
    fs::create_dir_all(&folder).unwrap();
    let mut job = Job::new("Exemple fictif.wav".into(), duration, chunk);
    let binding = job_binding(&job, id);
    let mut first = 0;
    process_chunks(
        &mut job,
        &folder.join("travail.json"),
        |index, start, duration| {
            let wav = folder.join(format!("tranche-{index:08}.wav"));
            fs::write(&wav, make_wave(index)).unwrap();
            let prepared = prepare(&wav, duration)?;
            let segments = vec![Segment {
                start_ms: 100,
                end_ms: 800,
                text: format!("Paroles fictives {index}."),
                speaker_id: None,
                translated_text: None,
            }];
            retain_receipt(
                &folder, index, start, duration, &binding, first, &segments, prepared,
            )?;
            first += segments.len();
            Ok(segments)
        },
    )
    .unwrap();
    dir
}

#[cfg(unix)]
#[test]
fn refuse_les_liens_meme_vers_des_preuves_et_octets_valides() {
    use std::os::unix::fs::symlink;
    for leaf in [
        "",
        ID,
        "travail.json",
        "tranche-00000001.audio.json",
        "tranche-00000001.wav",
    ] {
        let dir = fixture();
        let root = dir.path().join("jobs");
        let path = match leaf {
            "" => root.clone(),
            ID => root.join(ID),
            name => root.join(ID).join(name),
        };
        let real = dir.path().join("original");
        fs::rename(&path, &real).unwrap();
        symlink(&real, &path).unwrap();
        let result = AudioLibrary::open(&root).and_then(|store| store.load_at(ID, 1_250));
        assert!(result.is_err(), "Un lien ne doit pas être ouvert : {leaf}");
    }
}

#[test]
fn garde_la_racine_ouverte_malgre_son_remplacement_par_un_autre_dossier() {
    let dir = fixture();
    let root = dir.path().join("jobs");
    let library = AudioLibrary::open(&root).unwrap();
    fs::rename(&root, dir.path().join("jobs-originaux")).unwrap();
    fs::create_dir(&root).unwrap();
    assert_eq!(library.load_at(ID, 1_250).unwrap().bytes(), wave(1));
}

#[test]
fn refuse_un_nom_non_canonique_meme_si_le_producteur_la_signe() {
    let id = "pas-un-identifiant";
    let dir = fixture_named(id);
    let library = AudioLibrary::open(&dir.path().join("jobs")).unwrap();
    assert!(library.load_at(id, 1_250).is_err());
}

#[test]
fn refuse_les_liens_physiques_sur_les_trois_fichiers_lus() {
    for name in [
        "travail.json",
        "tranche-00000001.audio.json",
        "tranche-00000001.wav",
    ] {
        let dir = fixture();
        let root = dir.path().join("jobs");
        fs::hard_link(root.join(ID).join(name), dir.path().join("autre-acces")).unwrap();
        assert!(
            AudioLibrary::open(&root)
                .unwrap()
                .load_at(ID, 1_250)
                .is_err(),
            "Lien physique accepté : {name}"
        );
    }
}

#[test]
fn borne_la_lecture_du_descripteur_sans_tronquer_en_silence() {
    let dir = fixture();
    let library = AudioLibrary::open(&dir.path().join("jobs")).unwrap();
    let folder = library.root.child(ID).unwrap();
    let expected = fs::read(dir.path().join("jobs").join(ID).join("travail.json")).unwrap();
    assert_eq!(
        folder.read("travail.json", expected.len()).unwrap(),
        expected
    );
    assert!(folder.read("travail.json", expected.len() - 1).is_err());
}

#[test]
fn refuse_un_instant_apres_la_fin_reelle_meme_si_le_travail_le_contient() {
    let dir = fixture_options(ID, 2_200, 1_100);
    let library = AudioLibrary::open(&dir.path().join("jobs")).unwrap();
    assert!(library.load_at(ID, 2_099).is_ok());
    assert!(library.load_at(ID, 2_100).is_err());
}

#[test]
fn grammaire_identifiants_et_instants_extremes() {
    assert!(canonical_id(ID));
    assert!(canonical_id("abcdefab-abcd-4abc-8abc-abcdefabcdef"));
    for id in [
        "",
        "..",
        "../ailleurs",
        "/absolu",
        "C:\\ailleurs",
        "//serveur/partage",
        "11111111-1111-4111-8111-111111111111/..",
        "ABCDEFAB-ABCD-4ABC-8ABC-ABCDEFABCDEF",
        "11111111-1111-4111-8111-11111111111é",
        "11111111-1111-4111-8111-111111111111\0",
    ] {
        assert!(!canonical_id(id), "Identifiant accepté : {id:?}");
    }
    let dir = fixture();
    let library = AudioLibrary::open(&dir.path().join("jobs")).unwrap();
    assert_eq!(library.load_at(ID, 0).unwrap().bytes(), wave(0));
    assert_eq!(library.load_at(ID, 999).unwrap().bytes(), wave(0));
    assert_eq!(library.load_at(ID, 1_000).unwrap().bytes(), wave(1));
    for time in [2_000, u64::MAX] {
        assert!(library.load_at(ID, time).is_err());
    }
}

#[test]
fn relit_la_confirmation_persistante_et_ne_reutilise_pas_un_succes_ancien() {
    let dir = fixture();
    let library = AudioLibrary::open(&dir.path().join("jobs")).unwrap();
    assert!(library.load_at(ID, 1_250).is_ok());
    let path = dir.path().join("jobs").join(ID).join("travail.json");
    let mut job: Job = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    job.completed_chunks = 1;
    fs::write(&path, serde_json::to_vec(&job).unwrap()).unwrap();
    assert!(library.load_at(ID, 1_250).is_err());
    assert!(library.load_at(ID, 500).is_ok());
    job.chunk_ms = 0;
    fs::write(&path, serde_json::to_vec(&job).unwrap()).unwrap();
    assert!(library.load_at(ID, 500).is_err());
}

#[test]
fn ne_substitue_jamais_un_autre_travail_ou_un_media_sans_preuve() {
    for name in [
        "travail.json",
        "tranche-00000001.audio.json",
        "tranche-00000001.wav",
    ] {
        let dir = fixture();
        let root = dir.path().join("jobs");
        let file = root.join(ID).join(name);
        fs::remove_file(&file).unwrap();
        assert!(AudioLibrary::open(&root)
            .unwrap()
            .load_at(ID, 1_250)
            .is_err());
    }
    let dir = fixture();
    let root = dir.path().join("jobs");
    let other = "22222222-2222-4222-8222-222222222222";
    fs::rename(root.join(ID), root.join(other)).unwrap();
    assert!(AudioLibrary::open(&root)
        .unwrap()
        .load_at(other, 1_250)
        .is_err());
}

#[test]
fn rejette_la_corruption_et_ne_rouvre_pas_les_octets_une_fois_valides() {
    let dir = fixture();
    let root = dir.path().join("jobs");
    let library = AudioLibrary::open(&root).unwrap();
    let verified = library.load_at(ID, 1_250).unwrap();
    let mut altered = wave(1);
    *altered.last_mut().unwrap() ^= 1;
    fs::write(root.join(ID).join("tranche-00000001.wav"), altered).unwrap();
    assert_eq!(verified.into_bytes(), wave(1));
    assert!(library.load_at(ID, 1_250).is_err());
}

#[test]
fn garde_le_dossier_du_travail_ouvert_pendant_son_deplacement() {
    let dir = fixture();
    let root = dir.path().join("jobs");
    let library = AudioLibrary::open(&root).unwrap();
    let folder = library.root.child(ID).unwrap();
    fs::rename(root.join(ID), dir.path().join("ancien-travail")).unwrap();
    fs::create_dir(root.join(ID)).unwrap();
    assert_eq!(
        folder
            .read(
                "tranche-00000001.wav",
                crate::retained_audio::MAX_AUDIO_BYTES
            )
            .unwrap(),
        wave(1)
    );
    assert!(library.load_at(ID, 1_250).is_err());
}

#[cfg(unix)]
#[test]
fn refuse_un_tube_sans_attendre_un_producteur() {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let dir = fixture();
    let root = dir.path().join("jobs");
    let path = root.join(ID).join("tranche-00000001.wav");
    fs::remove_file(&path).unwrap();
    let path = CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: pathname owned by this temporary fixture, valid C string; no external data.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    assert!(AudioLibrary::open(&root)
        .unwrap()
        .load_at(ID, 1_250)
        .is_err());
}

#[test]
fn recupere_les_octets_du_producteur_depuis_le_travail_persistant() {
    let dir = fixture();
    let library = AudioLibrary::open(&dir.path().join("jobs")).unwrap();
    let audio = library.load_at(ID, 1_250).unwrap();
    assert_eq!(audio.bytes(), wave(1));
    assert_eq!(audio.timeline_offset_ms(), 1_000);
    assert_eq!(audio.duration_ms(), 1_000);
}

#[test]
fn planifie_la_tranche_sur_la_duree_demandee_pas_la_duree_wav() {
    let dir = fixture_options(ID, 2_200, 1_100);
    let library = AudioLibrary::open(&dir.path().join("jobs")).unwrap();
    let plan = library.plan_at(ID, 500).unwrap();
    if let Ok(path) = std::env::var("PAROLE_AUDIO_FIXTURE_OUTPUT") {
        fs::write(
            format!("{path}.gap"),
            crate::audio_playback::encode(&library, ID, 0).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(plan.audio.bytes(), wave(0));
    assert_eq!(plan.audio.duration_ms(), 1_000);
    assert_eq!(plan.playable_end_ms, 1_000);
    assert_eq!(plan.next_ms, Some(1_100));
    assert!(library.plan_at(ID, 1_050).is_err());
    assert_eq!(library.plan_at(ID, 1_100).unwrap().next_ms, None);
}

#[test]
fn borne_sans_recouvrement_meme_si_le_wav_depasse_la_duree_demandee() {
    let dir = fixture_options(ID, 1_800, 900);
    let library = AudioLibrary::open(&dir.path().join("jobs")).unwrap();
    let plan = library.plan_at(ID, 850).unwrap();
    assert_eq!(plan.audio.duration_ms(), 1_000);
    assert_eq!(plan.playable_end_ms, 900);
    assert_eq!(plan.next_ms, Some(900));
    let last = library.plan_at(ID, 900).unwrap();
    assert_eq!(last.audio.timeline_offset_ms(), 900);
    assert_eq!(last.playable_end_ms, 1_800);
    assert_eq!(last.next_ms, None);
    if let Ok(path) = std::env::var("PAROLE_AUDIO_FIXTURE_OUTPUT") {
        fs::write(
            format!("{path}.overlap-first"),
            crate::audio_playback::encode(&library, ID, 0).unwrap(),
        )
        .unwrap();
        fs::write(
            format!("{path}.overlap-last"),
            crate::audio_playback::encode(&library, ID, 900).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn enveloppe_wav_avec_blocs_auxiliaires_avant_et_apres_les_echantillons() {
    fn auxiliary_wave(index: usize) -> Vec<u8> {
        let original = wave(index);
        let mut bytes = original[..12].to_vec();
        bytes.extend_from_slice(b"JUNK");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(b"abc\0");
        bytes.extend_from_slice(&original[12..]);
        bytes.extend_from_slice(b"LIST");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(b"INFO");
        let size = u32::try_from(bytes.len() - 8).unwrap();
        bytes[4..8].copy_from_slice(&size.to_le_bytes());
        bytes
    }
    let dir = fixture_wave(ID, 1_800, 900, auxiliary_wave);
    let library = AudioLibrary::open(&dir.path().join("jobs")).unwrap();
    assert_eq!(
        library.plan_at(ID, 900).unwrap().audio.bytes(),
        auxiliary_wave(1)
    );
    if let Ok(path) = std::env::var("PAROLE_AUDIO_FIXTURE_OUTPUT") {
        fs::write(
            format!("{path}.auxiliary"),
            crate::audio_playback::encode(&library, ID, 900).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn enveloppes_navigation_depuis_les_instants_entiers_du_producteur() {
    let dir = fixture_options(ID, 20_000, 1_000);
    let library = AudioLibrary::open(&dir.path().join("jobs")).unwrap();
    for (suffix, at, origin) in [
        ("nav-first", 0, 0),
        ("nav-forward", 10_125, 10_000),
        ("nav-back", 125, 0),
    ] {
        let plan = library.plan_at(ID, at).unwrap();
        assert_eq!(plan.audio.timeline_offset_ms(), origin);
        if let Ok(path) = std::env::var("PAROLE_AUDIO_FIXTURE_OUTPUT") {
            fs::write(
                format!("{path}.{suffix}"),
                crate::audio_playback::encode(&library, ID, at).unwrap(),
            )
            .unwrap();
        }
    }
}

#[test]
fn enveloppe_binaire_depuis_le_producteur_et_refuse_sans_recu() {
    let dir = fixture_options(ID, 4_000, 1_000);
    let library = AudioLibrary::open(&dir.path().join("jobs")).unwrap();
    let packet = crate::audio_playback::encode(&library, ID, 1_250).unwrap();
    assert_eq!(&packet[..4], b"PAU1");
    assert_eq!(&packet[4..40], ID.as_bytes());
    assert_eq!(
        u64::from_le_bytes(packet[40..48].try_into().unwrap()),
        1_250
    );
    assert_eq!(
        u64::from_le_bytes(packet[48..56].try_into().unwrap()),
        1_000
    );
    assert_eq!(
        u64::from_le_bytes(packet[56..64].try_into().unwrap()),
        1_000
    );
    assert_eq!(
        u64::from_le_bytes(packet[64..72].try_into().unwrap()),
        2_000
    );
    assert_eq!(
        u64::from_le_bytes(packet[72..80].try_into().unwrap()),
        2_000
    );
    assert_eq!(
        u32::from_le_bytes(packet[80..84].try_into().unwrap()) as usize,
        wave(1).len()
    );
    assert_eq!(&packet[84..], wave(1));
    if let Ok(path) = std::env::var("PAROLE_AUDIO_FIXTURE_OUTPUT") {
        fs::write(&path, &packet).unwrap();
        fs::write(
            format!("{path}.first"),
            crate::audio_playback::encode(&library, ID, 0).unwrap(),
        )
        .unwrap();
        fs::write(
            format!("{path}.next"),
            crate::audio_playback::encode(&library, ID, 1_000).unwrap(),
        )
        .unwrap();
    }
    fs::remove_file(
        dir.path()
            .join("jobs")
            .join(ID)
            .join("tranche-00000001.audio.json"),
    )
    .unwrap();
    assert!(crate::audio_playback::encode(&library, ID, 1_250).is_err());
}
