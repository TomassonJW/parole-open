//! Tests de la diarisation locale.
//!
//! `lib.rs` n'est volontairement pas modifié pour ces tests : la diarisation
//! et son utilitaire de sous-processus sont chargés depuis leurs sources.
//!
//! Les tests d'inférence réelle utilisent des ressources natives hors dépôt,
//! dont le dossier est donné par `PAROLE_DIARIZATION_ASSETS` :
//!   lib/libsherpa-onnx-c-api.so (+ libonnxruntime.so), sherpa-onnx v1.13.x
//!   sherpa-onnx-pyannote-segmentation-3-0/model.onnx         (MIT)
//!   3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx        (Apache-2.0)
//!   2-two-speakers-en.wav, 0-four-speakers-zh.wav            (exemples sherpa-onnx)
//! Sans ce dossier ils sont ignorés (message explicite), sauf si
//! `PAROLE_REQUIRE_DIARIZATION=1`, auquel cas ils échouent.

#[path = "../src/child_process.rs"]
mod child_process;
#[path = "../src/diarization.rs"]
#[allow(dead_code)]
mod diarization;

use diarization::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------- purs ----

fn unit(v: &[f32]) -> Vec<f32> {
    normalized(v)
}

#[test]
fn registre_apparie_un_a_un_et_cree_de_nouveaux_locuteurs() {
    let mut reg = SpeakerRegistry::default();
    let a = unit(&[1.0, 0.0, 0.0]);
    let b = unit(&[0.0, 1.0, 0.0]);
    let ids = reg.match_embeddings(&[(a.clone(), 4000), (b.clone(), 3000)], 0.5);
    assert_eq!(ids, vec!["Locuteur 1", "Locuteur 2"]);
    // Tranche suivante, ordre local inversé, empreintes bruitées.
    let ids = reg.match_embeddings(
        &[
            (unit(&[0.1, 0.95, 0.0]), 2000),
            (unit(&[0.9, 0.1, 0.05]), 2000),
        ],
        0.5,
    );
    assert_eq!(ids, vec!["Locuteur 2", "Locuteur 1"]);
    // Deux locaux proches du même global : un seul est rattaché (un-à-un).
    let ids = reg.match_embeddings(&[(a.clone(), 1000), (unit(&[0.98, 0.02, 0.0]), 1000)], 0.5);
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        vec!["Locuteur 1", "Locuteur 3"],
        "un seul rattaché au même global"
    );
    // Voix inconnue sous le seuil.
    let ids = reg.match_embeddings(&[(unit(&[0.0, 0.0, 1.0]), 1000)], 0.5);
    assert_eq!(ids, vec!["Locuteur 4"]);
    assert_eq!(reg.speakers.len(), 4);
}

#[test]
fn registre_persiste_et_se_recharge() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("locuteurs.json");
    assert!(SpeakerRegistry::load(&path).unwrap().speakers.is_empty());
    let mut reg = SpeakerRegistry::default();
    reg.match_embeddings(&[(unit(&[1.0, 2.0]), 1500)], 0.5);
    reg.processed_chunks.push(0);
    reg.save(&path).unwrap();
    let back = SpeakerRegistry::load(&path).unwrap();
    assert_eq!(back.speakers.len(), 1);
    assert_eq!(back.speakers[0].id, "Locuteur 1");
    assert_eq!(back.processed_chunks, vec![0]);
    assert!(!path.with_extension("json.tmp").exists());
}

#[test]
fn attribution_par_recouvrement_maximal_sans_invention() {
    let turns = vec![
        SpeakerTurn {
            start_ms: 0,
            end_ms: 3000,
            speaker_id: "Locuteur 1".into(),
        },
        SpeakerTurn {
            start_ms: 2500,
            end_ms: 6000,
            speaker_id: "Locuteur 2".into(),
        },
    ];
    let got = assign_speakers(&[(0, 2000), (2000, 5000), (7000, 8000)], &turns);
    assert_eq!(got[0].as_deref(), Some("Locuteur 1"));
    assert_eq!(got[1].as_deref(), Some("Locuteur 2"));
    assert_eq!(got[2], None, "aucun tour : aucun locuteur inventé");
}

#[test]
fn cosinus_et_normalisation() {
    assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
    assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
    assert_eq!(cosine(&[1.0], &[1.0, 2.0]), -1.0);
    let n = normalized(&[3.0, 4.0]);
    assert!((n[0] - 0.6).abs() < 1e-6 && (n[1] - 0.8).abs() < 1e-6);
}

#[test]
fn ressources_absentes_donnent_une_erreur_explicite() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DiarizationConfig::new(
        dir.path().join("libsherpa-onnx-c-api.so"),
        dir.path().join("seg.onnx"),
        dir.path().join("emb.onnx"),
    );
    match Diarizer::new(cfg) {
        Err(DiarizationError::MissingModel(p)) => assert!(p.ends_with("seg.onnx")),
        Err(e) => panic!("erreur inattendue : {e}"),
        Ok(_) => panic!("aucun moteur ne doit être créé sans modèles"),
    }
    std::fs::write(dir.path().join("seg.onnx"), b"x").unwrap();
    std::fs::write(dir.path().join("emb.onnx"), b"x").unwrap();
    let cfg = DiarizationConfig::new(
        dir.path().join("absente.so"),
        dir.path().join("seg.onnx"),
        dir.path().join("emb.onnx"),
    );
    assert!(matches!(
        Diarizer::new(cfg),
        Err(DiarizationError::Library(_))
    ));
}

// ------------------------------------------------------ inférence réelle --

struct Assets {
    dir: PathBuf,
}

fn assets() -> Option<Assets> {
    let required = std::env::var("PAROLE_REQUIRE_DIARIZATION").ok().as_deref() == Some("1");
    let dir = std::env::var_os("PAROLE_DIARIZATION_ASSETS").map(PathBuf::from);
    let ok = dir.as_ref().is_some_and(|d| {
        d.join("lib/libsherpa-onnx-c-api.so").is_file()
            && d.join("sherpa-onnx-pyannote-segmentation-3-0/model.onnx")
                .is_file()
            && d.join("3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx")
                .is_file()
            && d.join("2-two-speakers-en.wav").is_file()
            && d.join("0-four-speakers-zh.wav").is_file()
    });
    if ok {
        return Some(Assets { dir: dir.unwrap() });
    }
    if required {
        panic!("PAROLE_REQUIRE_DIARIZATION=1 mais ressources natives absentes ({dir:?})");
    }
    eprintln!(
        "IGNORÉ : PAROLE_DIARIZATION_ASSETS non défini ou incomplet ; inférence réelle non testée"
    );
    None
}

impl Assets {
    fn config(&self) -> DiarizationConfig {
        DiarizationConfig::new(
            self.dir.join("lib/libsherpa-onnx-c-api.so"),
            self.dir
                .join("sherpa-onnx-pyannote-segmentation-3-0/model.onnx"),
            self.dir
                .join("3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx"),
        )
    }
    fn wav(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
}

fn ffmpeg() -> PathBuf {
    PathBuf::from("ffmpeg")
}

fn probe_ms(media: &Path) -> u64 {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(media)
        .output()
        .unwrap();
    (String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .unwrap()
        * 1000.0) as u64
}

/// Taux d'accord (sur la durée) entre deux étiquetages, après la meilleure
/// correspondance un-à-un gloutonne entre étiquettes.
fn agreement(a: &[SpeakerTurn], b: &[SpeakerTurn], duration_ms: u64) -> f64 {
    let step = 50;
    let label = |turns: &[SpeakerTurn], t: u64| -> Option<String> {
        let hits: Vec<&SpeakerTurn> = turns
            .iter()
            .filter(|x| t >= x.start_ms && t < x.end_ms)
            .collect();
        (hits.len() == 1).then(|| hits[0].speaker_id.clone())
    };
    let mut joint: BTreeMap<(String, String), u64> = BTreeMap::new();
    let mut total = 0u64;
    let mut t = 0;
    while t < duration_ms {
        if let (Some(x), Some(y)) = (label(a, t), label(b, t)) {
            *joint.entry((x, y)).or_default() += 1;
            total += 1;
        }
        t += step;
    }
    let mut pairs: Vec<_> = joint.into_iter().collect();
    pairs.sort_by_key(|p| std::cmp::Reverse(p.1));
    let (mut ua, mut ub, mut matched) = (Vec::new(), Vec::new(), 0u64);
    for ((x, y), n) in pairs {
        if !ua.contains(&x) && !ub.contains(&y) {
            matched += n;
            ua.push(x);
            ub.push(y);
        }
    }
    if total == 0 {
        0.0
    } else {
        matched as f64 / total as f64
    }
}

fn distinct(turns: &[SpeakerTurn]) -> Vec<String> {
    let mut v: Vec<String> = turns.iter().map(|t| t.speaker_id.clone()).collect();
    v.sort();
    v.dedup();
    v
}

#[test]
fn inference_reelle_deux_locuteurs_fichier_entier() {
    let Some(a) = assets() else { return };
    let d = Diarizer::new(a.config()).expect("moteur natif");
    assert!(d.version().starts_with("1.13."));
    let pcm = decode_pcm_f32(&ffmpeg(), &a.wav("2-two-speakers-en.wav"), 0, None).unwrap();
    assert!(pcm.len() > 16_000 * 20);
    let turns = d.diarize_samples(&pcm).unwrap();
    eprintln!("tours locaux : {turns:?}");
    // Durée par étiquette locale ; le moteur peut produire un micro-groupe
    // parasite (< 1 s) que la couche par tranches écarte ensuite.
    let mut per: BTreeMap<i32, f32> = BTreeMap::new();
    for t in &turns {
        *per.entry(t.local_speaker).or_default() += t.end_s - t.start_s;
    }
    let major = per.values().filter(|d| **d >= 1.0).count();
    assert_eq!(major, 2, "exemple public à deux voix : {per:?}");
    assert!(turns.len() >= 6, "alternances de parole attendues");
}

#[test]
fn inference_reelle_empreintes_discriminantes() {
    let Some(a) = assets() else { return };
    let d = Diarizer::new(a.config()).unwrap();
    let pcm = decode_pcm_f32(&ffmpeg(), &a.wav("2-two-speakers-en.wav"), 0, None).unwrap();
    let cut = |s: f32, e: f32| pcm[(s * 16_000.0) as usize..(e * 16_000.0) as usize].to_vec();
    // Tours du fichier d'exemple : A 0–2.7, B 2.8–7.8, A 7.9–11.1, B 11.3–13.9.
    let a1 = d.embed(&cut(0.1, 2.7)).unwrap();
    let b1 = d.embed(&cut(2.9, 7.7)).unwrap();
    let a2 = d.embed(&cut(7.9, 11.0)).unwrap();
    let b2 = d.embed(&cut(11.4, 13.8)).unwrap();
    assert_eq!(a1.len(), d.embedding_dim());
    let same = cosine(&a1, &a2).min(cosine(&b1, &b2));
    let diff = cosine(&a1, &b1).max(cosine(&a2, &b2)).max(cosine(&a1, &b2));
    eprintln!("similarité même voix ≥ {same:.3}, voix différentes ≤ {diff:.3}");
    assert!(same > diff + 0.1);
    assert!(
        same >= d.config().match_threshold,
        "même voix au-dessus du seuil"
    );
    assert!(
        diff < d.config().match_threshold,
        "voix différentes sous le seuil"
    );
}

#[test]
fn inference_reelle_identites_coherentes_entre_tranches_repetees() {
    // Le même dialogue est joué deux fois ; découpé en deux tranches de la
    // longueur exacte du dialogue, chaque voix doit retrouver son identité.
    let Some(a) = assets() else { return };
    let dir = tempfile::tempdir().unwrap();
    let doubled = dir.path().join("double.wav");
    let src = a.wav("2-two-speakers-en.wav");
    let st = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(&src)
        .arg("-i")
        .arg(&src)
        .args([
            "-filter_complex",
            "[0:a][1:a]concat=n=2:v=0:a=1",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-y",
        ])
        .arg(&doubled)
        .status()
        .unwrap();
    assert!(st.success());
    let one = probe_ms(&src);
    let two = probe_ms(&doubled);
    let d = Diarizer::new(a.config()).unwrap();
    let mut reg = SpeakerRegistry::default();
    let turns = d
        .diarize_media_chunked(&ffmpeg(), &doubled, two, one, &mut reg)
        .unwrap();
    let first: Vec<SpeakerTurn> = turns.iter().filter(|t| t.start_ms < one).cloned().collect();
    let second: Vec<SpeakerTurn> = turns
        .iter()
        .filter(|t| t.start_ms >= one)
        .map(|t| SpeakerTurn {
            start_ms: t.start_ms - one,
            end_ms: t.end_ms - one,
            speaker_id: t.speaker_id.clone(),
        })
        .collect();
    eprintln!(
        "registre : {:?}",
        reg.speakers
            .iter()
            .map(|s| (&s.id, s.total_ms))
            .collect::<Vec<_>>()
    );
    assert_eq!(distinct(&first).len(), 2);
    assert_eq!(
        distinct(&first),
        distinct(&second),
        "mêmes identifiants globaux dans les deux tranches"
    );
    assert_eq!(
        reg.speakers.len(),
        2,
        "aucun locuteur fantôme créé à la seconde tranche"
    );
    // Identité littérale (pas seulement à permutation près) :
    let same_label = |t: u64| {
        let f = first
            .iter()
            .find(|x| t >= x.start_ms && t < x.end_ms)
            .map(|x| &x.speaker_id);
        let s = second
            .iter()
            .find(|x| t >= x.start_ms && t < x.end_ms)
            .map(|x| &x.speaker_id);
        (f, s)
    };
    for probe in [1_000u64, 5_000, 9_000, 12_500] {
        let (f, s) = same_label(probe);
        assert!(f.is_some() && f == s, "à {probe} ms : {f:?} ≠ {s:?}");
    }
}

#[test]
fn inference_reelle_tranches_courtes_concordent_avec_le_fichier_entier() {
    let Some(a) = assets() else { return };
    let d = Diarizer::new(a.config()).unwrap();
    let media = a.wav("2-two-speakers-en.wav");
    let duration = probe_ms(&media);
    let mut whole_reg = SpeakerRegistry::default();
    let whole = d
        .diarize_media_chunked(&ffmpeg(), &media, duration, duration, &mut whole_reg)
        .unwrap();
    let mut reg = SpeakerRegistry::default();
    let chunked = d
        .diarize_media_chunked(&ffmpeg(), &media, duration, 10_000, &mut reg)
        .unwrap();
    let acc = agreement(&whole, &chunked, duration);
    eprintln!(
        "accord tranches de 10 s / fichier entier : {:.1} % ; locuteurs {}",
        acc * 100.0,
        reg.speakers.len()
    );
    assert_eq!(distinct(&chunked).len(), 2, "deux voix sur trois tranches");
    assert!(acc >= 0.85, "accord insuffisant : {acc}");
}

#[test]
fn inference_reelle_reprise_conserve_les_identifiants() {
    let Some(a) = assets() else { return };
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("locuteurs.json");
    let media = a.wav("2-two-speakers-en.wav");
    let d = Diarizer::new(a.config()).unwrap();
    let pcm = decode_pcm_f32(&ffmpeg(), &media, 0, None).unwrap();
    let half = 13_500usize;
    let first_pcm = &pcm[..half * 16];
    let second_pcm = &pcm[half * 16..];

    let mut reg = SpeakerRegistry::default();
    let t0 = d.diarize_chunk(&mut reg, 0, 0, first_pcm).unwrap();
    reg.save(&state).unwrap();
    drop(d);

    // « Redémarrage » : nouveau moteur, registre rechargé.
    let d = Diarizer::new(a.config()).unwrap();
    let mut reg = SpeakerRegistry::load(&state).unwrap();
    let before = reg.speakers.len();
    let t1 = d
        .diarize_chunk(&mut reg, 1, half as u64, second_pcm)
        .unwrap();
    // Rejouer la tranche 0 après reprise ne modifie ni identités ni registre.
    let replay = d.diarize_chunk(&mut reg, 0, 0, first_pcm).unwrap();
    assert_eq!(
        replay, t0,
        "résultat identique et déterministe à la reprise"
    );
    eprintln!(
        "tranche 0 : {:?}\ntranche 1 : {:?}",
        distinct(&t0),
        distinct(&t1)
    );
    assert_eq!(before, 2);
    assert_eq!(reg.speakers.len(), 2);
    assert!(distinct(&t1).iter().all(|id| distinct(&t0).contains(id)));
}

#[test]
fn inference_reelle_quatre_locuteurs_par_tranches() {
    // Exemple public à quatre voix (mandarin) : le regroupement non
    // supervisé ne doit pas fusionner tout le monde ni exploser.
    let Some(a) = assets() else { return };
    let d = Diarizer::new(a.config()).unwrap();
    let media = a.wav("0-four-speakers-zh.wav");
    let duration = probe_ms(&media);
    let mut reg = SpeakerRegistry::default();
    let turns = d
        .diarize_media_chunked(&ffmpeg(), &media, duration, 20_000, &mut reg)
        .unwrap();
    let n = distinct(&turns).len();
    eprintln!("quatre voix, tranches de 20 s → {n} locuteurs globaux : {turns:?}");
    assert!((3..=6).contains(&n), "{n} locuteurs");
}
