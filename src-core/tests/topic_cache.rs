use parole_core::{topic_cache::TopicCache, topic_candidates::prepare_topic_candidates, Segment};
use std::fs;
const ID: &str = "11111111-1111-4111-8111-111111111111";
fn source() -> Vec<Segment> {
    let mut one = Segment::new(0, 1_000, "Le dossier Atlas prévoit un budget.".into());
    one.speaker_id = Some("voix-1".into());
    vec![
        one,
        Segment::new(2_000, 3_000, "Le budget reste à revoir.".into()),
        Segment::new(4_000, 5_000, "oui".into()),
    ]
}
fn saved_path(dir: &std::path::Path) -> std::path::PathBuf {
    fs::read_dir(dir).unwrap().next().unwrap().unwrap().path()
}
fn rewrite_candidates(path: &std::path::Path, change: impl FnOnce(&mut serde_json::Value)) {
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    change(&mut value["candidates"]);
    let typed: parole_core::topic_candidates::TopicCandidates =
        serde_json::from_value(value["candidates"].clone()).unwrap();
    value["candidates_sha256"] = serde_json::json!(parole_core::language::sha256_hex(
        &serde_json::to_vec(&typed).unwrap()
    ));
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}
#[test]
fn refuse_une_preuve_qui_ne_correspond_pas_a_la_source_meme_avec_son_empreinte() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("exploration");
    let cache = TopicCache::new(&dir);
    let segments = source();
    cache.prepare(ID, &segments).unwrap();
    let path = saved_path(&dir);
    rewrite_candidates(&path, |v| {
        v["words"][0]["evidence"][0]["citation"] = serde_json::json!("Paroles inventées")
    });
    let corrupt = fs::read(&path).unwrap();
    assert!(cache.load(ID, &segments).is_err());
    assert!(cache.prepare(ID, &segments).is_err());
    assert_eq!(
        fs::read(path).unwrap(),
        corrupt,
        "Ne pas masquer une corruption par un recalcul"
    );
}
#[test]
fn refuse_un_passage_sans_suggestion_qui_a_pourtant_un_dossier() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("exploration");
    let cache = TopicCache::new(&dir);
    let segments = vec![Segment::new(0, 1_000, "Dossier AB".into())];
    assert!(cache
        .prepare(ID, &segments)
        .unwrap()
        .without_suggestion
        .is_empty());
    rewrite_candidates(&saved_path(&dir), |v| {
        v["without_suggestion"] = serde_json::json!([0])
    });
    assert!(cache.load(ID, &segments).is_err());
}
#[test]
fn refuse_une_identite_non_canonique_avant_toute_ecriture() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("exploration");
    let cache = TopicCache::new(&dir);
    for id in [
        "",
        "../travail",
        "11111111-1111-4111-8111-11111111111A",
        "11111111-1111-4111-8111-111111111111/x",
    ] {
        assert!(
            cache.load(id, &source()).is_err(),
            "identité invalide acceptée"
        );
        assert!(cache.prepare(id, &source()).is_err());
        assert!(!dir.exists());
    }
}
#[test]
fn refuse_les_incoherences_structurelles_meme_si_le_json_est_lisible() {
    let mutations: Vec<fn(&mut serde_json::Value)> = vec![
        |v| v["schema_version"] = serde_json::json!(2),
        |v| v["words"][0]["term"] = serde_json::json!("autre-sujet"),
        |v| v["words"][0]["lexical_weight"] = serde_json::json!(0),
        |v| v["words"][0]["evidence"] = serde_json::json!([]),
        |v| v["links"] = serde_json::json!([]),
        |v| v["possible_folders"][0]["name"] = serde_json::json!("Luciole"),
        |v| {
            let first = v["words"][0].clone();
            v["words"].as_array_mut().unwrap().push(first);
        },
    ];
    for (index, change) in mutations.into_iter().enumerate() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("exploration");
        let cache = TopicCache::new(&dir);
        cache.prepare(ID, &source()).unwrap();
        rewrite_candidates(&saved_path(&dir), change);
        assert!(
            cache.load(ID, &source()).is_err(),
            "mutation {index} acceptée"
        );
    }
}
#[test]
fn refuse_les_sources_hors_budget_sans_aucune_ecriture() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("exploration");
    let cache = TopicCache::new(&dir);
    let mut translated = source();
    translated[0].translated_text = Some("x".repeat(16_385));
    for segments in [
        vec![Segment::new(0, 1, "x".repeat(4_097))],
        vec![Segment::new(0, 1, "oui".into()); 2_001],
        translated,
    ] {
        assert!(cache.load(ID, &segments).is_err());
        assert!(cache.prepare(ID, &segments).is_err());
        assert!(!dir.exists());
    }
}
#[test]
fn chaque_changement_de_source_invalide_le_reemploi_sans_effacer_l_ancienne_revision() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("exploration");
    let cache = TopicCache::new(&dir);
    let original = source();
    let result = cache.prepare(ID, &original).unwrap();
    let path = saved_path(&dir);
    let before = fs::read(&path).unwrap();
    let changes: Vec<fn(&mut Vec<Segment>)> = vec![
        |s| s[0].text.push('!'),
        |s| s[0].start_ms += 1,
        |s| s[0].end_ms += 1,
        |s| s[0].speaker_id = Some("voix-2".into()),
        |s| s[0].translated_text = Some("A new translation".into()),
        |s| s.swap(0, 1),
        |s| {
            s.pop();
        },
    ];
    for change in changes {
        let mut changed = original.clone();
        change(&mut changed);
        assert_eq!(cache.load(ID, &changed).unwrap(), None);
        assert_eq!(fs::read(&path).unwrap(), before);
    }
    assert_eq!(
        cache
            .load("22222222-2222-4222-8222-222222222222", &original)
            .unwrap(),
        None
    );
    assert_eq!(cache.load(ID, &original).unwrap(), Some(result));
    let mut changed = original.clone();
    changed[0].text.push('!');
    cache.prepare(ID, &changed).unwrap();
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
    assert_eq!(fs::read(&path).unwrap(), before);
}
#[test]
fn refuse_corruption_version_inconnue_et_mauvaise_revision_sans_reecriture() {
    let changes: Vec<fn(&mut serde_json::Value)> = vec![
        |v| v["schema_version"] = serde_json::json!(2),
        |v| v["producer_revision"] = serde_json::json!(99),
        |v| v["source_revision"] = serde_json::json!("00".repeat(32)),
        |v| v["candidates_sha256"] = serde_json::json!("00".repeat(32)),
        |v| v["inconnu"] = serde_json::json!(true),
    ];
    for change in changes {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("exploration");
        let cache = TopicCache::new(&dir);
        cache.prepare(ID, &source()).unwrap();
        let path = saved_path(&dir);
        let mut value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        change(&mut value);
        let corrupt = serde_json::to_vec(&value).unwrap();
        fs::write(&path, &corrupt).unwrap();
        assert!(cache.load(ID, &source()).is_err());
        assert!(cache.prepare(ID, &source()).is_err());
        assert_eq!(fs::read(&path).unwrap(), corrupt);
    }
}
#[test]
fn conserve_les_annotations_humaines_et_les_paroles_a_cote_du_cache() {
    let temp = tempfile::tempdir().unwrap();
    let annotations = temp.path().join("annotations-v1");
    fs::create_dir(&annotations).unwrap();
    let correction = annotations.join("choix-humains.json");
    fs::write(&correction, b"{\"classement_humain\":true}").unwrap();
    let source_path = temp.path().join("travail.json");
    let serialized = serde_json::to_vec(&source()).unwrap();
    fs::write(&source_path, &serialized).unwrap();
    let cache = TopicCache::new(&temp.path().join("exploration"));
    let loaded: Vec<Segment> = serde_json::from_slice(&fs::read(&source_path).unwrap()).unwrap();
    cache.prepare(ID, &loaded).unwrap();
    assert_eq!(fs::read(&source_path).unwrap(), serialized);
    assert_eq!(
        fs::read(correction).unwrap(),
        b"{\"classement_humain\":true}"
    );
}
#[test]
fn accepte_les_frontieres_utf8_et_un_resultat_vide_sans_inventer_de_theme() {
    let temp = tempfile::tempdir().unwrap();
    let cache = TopicCache::new(&temp.path().join("exploration"));
    for segments in [
        vec![],
        vec![Segment::new(
            0,
            1000,
            "Le dossier AB. Budget. Éclairage. e\u{301}clairage. A03 et A030.".into(),
        )],
    ] {
        let result = cache.prepare(ID, &segments).unwrap();
        assert_eq!(cache.load(ID, &segments).unwrap(), Some(result));
    }
}
#[test]
fn refuse_un_fichier_trop_grand_ou_tronque_sans_le_lire_comme_valide() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("exploration");
    let cache = TopicCache::new(&dir);
    cache.prepare(ID, &source()).unwrap();
    let path = saved_path(&dir);
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(32 * 1024 * 1024 + 1)
        .unwrap();
    assert!(cache.load(ID, &source()).is_err());
    assert!(cache.prepare(ID, &source()).is_err());
    fs::write(&path, b"{\"schema_version\":1,").unwrap();
    assert!(cache.load(ID, &source()).is_err());
}
#[cfg(unix)]
#[test]
fn refuse_liens_symboliques_physiques_et_non_fichiers() {
    use std::os::unix::fs::symlink;
    for mode in ["symlink", "dangling", "hardlink", "directory", "root"] {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("exploration");
        let cache = TopicCache::new(&dir);
        cache.prepare(ID, &source()).unwrap();
        let file = saved_path(&dir);
        let kept = temp.path().join("preuve-valide.json");
        fs::rename(&file, &kept).unwrap();
        let before = fs::read(&kept).unwrap();
        match mode {
            "symlink" => symlink(&kept, &file).unwrap(),
            "dangling" => symlink(temp.path().join("absent"), &file).unwrap(),
            "hardlink" => fs::hard_link(&kept, &file).unwrap(),
            "directory" => fs::create_dir(&file).unwrap(),
            "root" => {
                fs::rename(&dir, temp.path().join("vrai-dossier")).unwrap();
                symlink(temp.path().join("vrai-dossier"), &dir).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(cache.load(ID, &source()).is_err(), "{mode}");
        assert!(cache.prepare(ID, &source()).is_err(), "{mode}");
        assert_eq!(fs::read(&kept).unwrap(), before);
    }
}
#[test]
fn conserve_le_resultat_reel_et_le_relit_sans_preparation() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("exploration");
    let segments = source();
    let expected = prepare_topic_candidates(&segments).unwrap();
    let cache = TopicCache::new(&dir);
    assert_eq!(cache.load(ID, &segments).unwrap(), None);
    assert!(!dir.exists(), "Consulter ne doit rien créer");
    assert_eq!(cache.prepare(ID, &segments).unwrap(), expected);
    let before = fs::read_dir(&dir)
        .unwrap()
        .map(|p| {
            let p = p.unwrap().path();
            (p.clone(), fs::read(p).unwrap())
        })
        .collect::<Vec<_>>();
    drop(cache);
    let reopened = TopicCache::new(&dir);
    assert_eq!(
        reopened.load(ID, &segments).unwrap(),
        Some(expected.clone())
    );
    assert_eq!(reopened.prepare(ID, &segments).unwrap(), expected);
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
    assert_eq!(fs::read(&before[0].0).unwrap(), before[0].1);
}
