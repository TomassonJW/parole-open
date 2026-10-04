use parole_core::topic_selection::{
    select_topics, CandidateScore, ReviewState, ScoreBand, SelectionThresholds,
};

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}
fn score(id: &str, value: f64) -> CandidateScore {
    CandidateScore {
        candidate_id: id.to_owned(),
        score: value,
    }
}
fn thresholds() -> SelectionThresholds {
    // Valeurs synthétiques de tests, pas une calibration du modèle.
    SelectionThresholds {
        uncertain_from: 0.4,
        proposed_from: 0.8,
    }
}

#[test]
fn plusieurs_scores_forts_restent_des_propositions_dans_l_ordre_initial() {
    let candidates = ids(&["sujet-b", "sujet-a"]);
    let scores = vec![score("sujet-b", 0.95), score("sujet-a", 0.95)];
    let before = scores.clone();
    let result = select_topics(thresholds(), &candidates, &scores).unwrap();
    assert_eq!(result.assessments.len(), 2);
    assert_eq!(result.state, ReviewState::Proposed);
    assert_eq!(result.assessments[0].candidate_id, "sujet-b");
    assert_eq!(result.assessments[1].candidate_id, "sujet-a");
    assert!(result
        .assessments
        .iter()
        .all(|entry| entry.band == ScoreBand::Proposed));
    assert_eq!(result.assessments[0].score, 0.95);
    assert_eq!(scores, before);
}

#[test]
fn aucun_score_suffisant_ne_force_un_gagnant() {
    let candidates = ids(&["sujet-a", "sujet-b"]);
    let scores = vec![score("sujet-a", 0.1), score("sujet-b", 0.39)];
    let result = select_topics(thresholds(), &candidates, &scores).unwrap();
    assert_eq!(result.state, ReviewState::NoSuggestion);
    assert_eq!(result.assessments.len(), 2);
    assert!(result
        .assessments
        .iter()
        .all(|entry| entry.band == ScoreBand::BelowThreshold));
    assert_eq!(result.assessments[1].score, 0.39);
}

#[test]
fn une_zone_incertaine_reste_ambigue_meme_a_cote_d_une_proposition_forte() {
    let candidates = ids(&["fort", "incertain", "faible"]);
    let scores = vec![
        score("fort", 0.99),
        score("incertain", 0.6),
        score("faible", 0.01),
    ];
    let result = select_topics(thresholds(), &candidates, &scores).unwrap();
    assert_eq!(result.state, ReviewState::Ambiguous);
    let bands: Vec<_> = result.assessments.iter().map(|entry| entry.band).collect();
    assert_eq!(
        bands,
        vec![
            ScoreBand::Proposed,
            ScoreBand::Uncertain,
            ScoreBand::BelowThreshold
        ]
    );
    assert_eq!(result.assessments[1].score, 0.6);
}

#[test]
fn une_liste_vide_signale_qu_il_manque_des_candidats() {
    let result = select_topics(thresholds(), &[], &[]).unwrap();
    assert_eq!(result.state, ReviewState::NoCandidates);
    assert!(result.assessments.is_empty());
    assert_eq!(result.thresholds, thresholds());
}

#[test]
fn les_seuils_invalides_sont_refuses_meme_sans_candidat() {
    use parole_core::topic_selection::SelectionError;
    let invalid = [
        (f64::NAN, 0.8),
        (0.4, f64::NAN),
        (f64::INFINITY, 0.8),
        (0.4, f64::NEG_INFINITY),
        (-0.1, 0.8),
        (0.4, 1.1),
        (0.8, 0.4),
        (0.4, 0.4),
        (1.0, 1.0),
        (0.0, 0.0),
    ];
    for (uncertain_from, proposed_from) in invalid {
        let policy = SelectionThresholds {
            uncertain_from,
            proposed_from,
        };
        assert_eq!(
            select_topics(policy, &[], &[]),
            Err(SelectionError::InvalidThresholds)
        );
    }
}

#[test]
fn des_identifiants_non_canoniques_ou_dupliques_ne_sont_pas_repares() {
    use parole_core::topic_selection::SelectionError;
    for invalid in ["", "   ", " sujet", "sujet ", "sujet\n", "a\0b", "a\tb"] {
        let candidates = ids(&[invalid]);
        assert_eq!(
            select_topics(thresholds(), &candidates, &[score(invalid, 0.99)]),
            Err(SelectionError::InvalidCandidates)
        );
        assert_eq!(candidates[0], invalid);
    }
    let long = "é".repeat(129);
    assert_eq!(
        select_topics(
            thresholds(),
            std::slice::from_ref(&long),
            &[score(&long, 0.99)]
        ),
        Err(SelectionError::InvalidCandidates)
    );
    assert_eq!(
        select_topics(
            thresholds(),
            &ids(&["même", "même"]),
            &[score("même", 0.9), score("même", 0.91)]
        ),
        Err(SelectionError::InvalidCandidates)
    );
}

#[test]
fn les_scores_incomplets_ajoutes_permutés_ou_d_un_autre_sujet_sont_refuses() {
    use parole_core::topic_selection::SelectionError;
    let candidates = ids(&["b", "a"]);
    let invalid = [
        vec![score("b", 0.9)],
        vec![score("b", 0.9), score("a", 0.8), score("c", 1.0)],
        vec![score("a", 0.8), score("b", 0.9)],
        vec![score("b", 0.9), score("c", 0.8)],
        vec![score("b", 0.9), score("b", 0.8)],
    ];
    for scores in invalid {
        assert_eq!(
            select_topics(thresholds(), &candidates, &scores),
            Err(SelectionError::CandidateMismatch)
        );
    }
    assert_eq!(
        select_topics(thresholds(), &[], &[score("a", 1.0)]),
        Err(SelectionError::CandidateMismatch)
    );
}

#[test]
fn un_score_invalide_refuse_tout_le_lot_sans_resultat_partiel() {
    use parole_core::topic_selection::SelectionError;
    let candidates = ids(&["premier", "dernier"]);
    for invalid in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.1,
        -f64::MIN_POSITIVE,
        f64::from_bits(1.0_f64.to_bits() + 1),
    ] {
        let scores = vec![score("premier", 0.99), score("dernier", invalid)];
        let before: Vec<_> = scores
            .iter()
            .map(|entry| (entry.candidate_id.clone(), entry.score.to_bits()))
            .collect();
        assert_eq!(
            select_topics(thresholds(), &candidates, &scores),
            Err(SelectionError::InvalidScore)
        );
        let after: Vec<_> = scores
            .iter()
            .map(|entry| (entry.candidate_id.clone(), entry.score.to_bits()))
            .collect();
        assert_eq!(after, before);
    }
}

#[test]
fn un_score_maximal_serialise_une_proposition_a_examiner_pas_une_confirmation() {
    let result = select_topics(
        thresholds(),
        &ids(&["proposition"]),
        &[score("proposition", 1.0)],
    )
    .unwrap();
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "thresholds": { "uncertain_from": 0.4, "proposed_from": 0.8 },
            "review_state": "proposed",
            "assessments": [{ "candidate_id": "proposition", "score": 1.0, "band": "proposed" }],
        })
    );
    assert_eq!(result.state, ReviewState::Proposed);
}

#[test]
fn les_frontieres_sont_inclusives_et_aucun_arrondi_ne_change_une_bande() {
    let policy = thresholds();
    let values = [
        f64::from_bits(policy.uncertain_from.to_bits() - 1),
        policy.uncertain_from,
        f64::from_bits(policy.proposed_from.to_bits() - 1),
        policy.proposed_from,
        f64::from_bits(policy.proposed_from.to_bits() + 1),
    ];
    let candidates: Vec<_> = (0..values.len()).map(|i| format!("id-{i}")).collect();
    let scores: Vec<_> = candidates
        .iter()
        .zip(values)
        .map(|(id, value)| score(id, value))
        .collect();
    let result = select_topics(policy, &candidates, &scores).unwrap();
    let bands: Vec<_> = result.assessments.iter().map(|entry| entry.band).collect();
    assert_eq!(
        bands,
        vec![
            ScoreBand::BelowThreshold,
            ScoreBand::Uncertain,
            ScoreBand::Uncertain,
            ScoreBand::Proposed,
            ScoreBand::Proposed
        ]
    );
    let bits: Vec<_> = result
        .assessments
        .iter()
        .map(|entry| entry.score.to_bits())
        .collect();
    assert_eq!(bits, values.map(f64::to_bits));
}

#[test]
fn les_seuils_fournis_sont_appliques_et_conserves_sans_valeur_cachee() {
    let candidates = ids(&["sujet"]);
    let scores = [score("sujet", 0.7)];
    let first = select_topics(thresholds(), &candidates, &scores).unwrap();
    assert_eq!(first.state, ReviewState::Ambiguous);
    let alternate = SelectionThresholds {
        uncertain_from: 0.2,
        proposed_from: 0.6,
    };
    let second = select_topics(alternate, &candidates, &scores).unwrap();
    assert_eq!(second.state, ReviewState::Proposed);
    assert_eq!(second.thresholds, alternate);
    assert_eq!(
        second.assessments[0].score.to_bits(),
        scores[0].score.to_bits()
    );
}

#[test]
fn identifiants_unicode_reserves_et_limite_exacte_restent_inchanges() {
    let max_bytes = "é".repeat(128);
    assert_eq!(max_bytes.len(), 256);
    let candidates = ids(&[
        "é",
        "e\u{301}",
        "__proto__",
        "constructor",
        "toString",
        "groupe: budget",
        &max_bytes,
    ]);
    let scores: Vec<_> = candidates.iter().map(|id| score(id, 0.99)).collect();
    let result = select_topics(thresholds(), &candidates, &scores).unwrap();
    let actual: Vec<_> = result
        .assessments
        .iter()
        .map(|entry| entry.candidate_id.clone())
        .collect();
    assert_eq!(actual, candidates);
    assert_eq!(result.state, ReviewState::Proposed);
}

#[test]
fn les_extremes_valides_ne_sont_pas_rejetes_et_zero_negatif_est_preserve() {
    let candidates = ids(&["nul", "plein"]);
    let scores = [score("nul", -0.0), score("plein", 1.0)];
    let policy = SelectionThresholds {
        uncertain_from: 0.0,
        proposed_from: 1.0,
    };
    let result = select_topics(policy, &candidates, &scores).unwrap();
    assert_eq!(result.state, ReviewState::Ambiguous);
    assert_eq!(result.assessments[0].band, ScoreBand::Uncertain);
    assert_eq!(result.assessments[0].score.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(result.assessments[1].band, ScoreBand::Proposed);
}
