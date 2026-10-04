use parole_core::{
    report_models::{report_model, report_models},
    Job,
};

#[test]
fn retired_metadata_is_preserved_but_generation_and_download_are_refused() {
    use parole_core::report_models::{new_report_models, report_model_for_generation};
    let retired = report_model("qwen3-4b-instruct-2507-q4_k_m").unwrap();
    assert!(!retired.available_for_new_jobs);
    assert_eq!(retired.bytes, 2_497_280_736);
    assert!(report_model_for_generation(&retired.id)
        .unwrap_err()
        .contains("désactivée"));
    let active = new_report_models().unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, "baseline");
    assert!(report_model_for_generation("baseline").is_ok());
}

#[test]
fn model_without_explicit_permission_is_not_available_for_new_jobs() {
    let mut raw = serde_json::to_value(report_model("baseline").unwrap()).unwrap();
    raw.as_object_mut()
        .unwrap()
        .remove("available_for_new_jobs");
    let model: parole_core::report_models::ReportModel = serde_json::from_value(raw).unwrap();
    assert!(!model.available_for_new_jobs);
}

#[test]
fn catalogue_epingle_et_selection_refuse_identifiants_inconnus_hors_ligne() {
    let all = report_models().unwrap();
    assert_eq!(all.len(), 2);
    let baseline = report_model("baseline").unwrap();
    let candidate = report_model("qwen3-4b-instruct-2507-q4_k_m").unwrap();
    assert_eq!(
        baseline.sha256,
        parole_core::language::MODELE_TEXTE_RECOMMANDE.sha256
    );
    assert_eq!(candidate.bytes, 2_497_280_736);
    assert_eq!(
        candidate.sha256,
        "2fde00ce69dd4899c70d020845e2638353015bba0fdf161b3eb965f2bca4464e"
    );
    assert!(candidate
        .url
        .contains("/ae44f08e1392f39c0e474af10c3ff8355c8b6688/"));
    assert_eq!(candidate.spec().size_bytes, candidate.bytes);
    assert!(report_model("unknown").is_err());
    assert!(report_model("../baseline").is_err());
}

#[test]
fn empreinte_sha_du_fichier_gguf_et_non_son_identifiant_xet() {
    let model = report_model("qwen3-4b-instruct-2507-q4_k_m").unwrap();
    // SHA-256 du fichier complet : l'identifiant xetHash du catalogue distant
    // est différent et ne peut pas servir à vérifier un téléchargement.
    assert_eq!(
        model.sha256,
        "2fde00ce69dd4899c70d020845e2638353015bba0fdf161b3eb965f2bca4464e"
    );
    assert_ne!(
        model.sha256,
        "497a8cc6e20c0cad0f441c9c0503e1624f45447c7310f84eb463aec9ad14f365"
    );
}

#[test]
fn catalogue_rejette_urls_non_epinglees() {
    use parole_core::report_models::pinned_model_url;
    assert!(pinned_model_url(
        &report_model("qwen3-4b-instruct-2507-q4_k_m").unwrap()
    ));
    let mut model = report_model("qwen3-4b-instruct-2507-q4_k_m").unwrap();
    model.url = "https://huggingface.co/bartowski/model/resolve/main/model.gguf".into();
    assert!(!pinned_model_url(&model));
    model.url = "https://huggingface.co/bartowski/model/resolve/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/other.gguf".into();
    assert!(!pinned_model_url(&model));
}

#[test]
fn verification_locale_refuse_taille_et_sha_incorrects_sans_reseau() {
    use parole_core::language::{sha256_hex, verify_model_file};
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("modele-test.gguf");
    assert!(verify_model_file(&file, &report_model("baseline").unwrap().spec()).is_err());
    std::fs::write(&file, b"local").unwrap();
    let mut model = report_model("qwen3-4b-instruct-2507-q4_k_m").unwrap();
    assert!(verify_model_file(&file, &model.spec()).is_err());
    model.bytes = 5;
    assert!(verify_model_file(&file, &model.spec()).is_err());
    model.sha256 = sha256_hex(b"local");
    assert!(verify_model_file(&file, &model.spec()).is_ok());
}

#[test]
fn saved_report_model_round_trips_and_legacy_defaults_to_baseline() {
    let mut job = Job::new("meeting.wav".into(), 30_000, 30_000);
    job.report_model_id = "qwen3-4b-instruct-2507-q4_k_m".into();
    let json = serde_json::to_value(&job).unwrap();
    assert_eq!(json["report_model_id"], "qwen3-4b-instruct-2507-q4_k_m");
    assert_eq!(
        serde_json::from_value::<Job>(json.clone())
            .unwrap()
            .report_model_id,
        job.report_model_id
    );
    let mut legacy = json;
    legacy.as_object_mut().unwrap().remove("report_model_id");
    assert_eq!(
        serde_json::from_value::<Job>(legacy)
            .unwrap()
            .report_model_id,
        "baseline"
    );
}
