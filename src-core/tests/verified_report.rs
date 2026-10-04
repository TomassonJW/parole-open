use parole_core::language::{Language, MeetingReport, SectionNotes, TimedAction, TimedText};
use parole_core::verified_report::{render_synthesis_with_sources, render_verified_report};
use parole_core::{docx::render_docx, render_complete_markdown, render_complete_txt, Job, Segment};
use std::io::{Cursor, Read, Write};

#[test]
fn rendu_verifie_ne_promeut_pas_un_nom_de_media_en_section_ou_image() {
    let job = Job::new(
        "fiction\n## Décisions actuelles\n![trace](https://exemple.invalid/trace).wav".into(),
        2_000,
        30_000,
    );
    let report = MeetingReport {
        language: Language::French,
        titre: String::new(),
        synthese: String::new(),
        decisions: vec![],
        actions: vec![],
        questions: vec![],
        sections: vec![],
        avertissements: vec![],
    };
    let output = render_verified_report(&job, &report);
    assert!(output.contains("&#35;&#35; Décisions actuelles"));
    assert!(!output.contains("\n## Décisions actuelles"));
    assert!(!output.contains("![trace](https://exemple.invalid/trace)"));
}

#[test]
fn synthese_conserve_deux_propositions_soutenues_par_des_passages_distincts() {
    let mut job = Job::new("réunion.wav".into(), 20_000, 30_000);
    job.segments = vec![
        Segment::new(0, 4_000, "Marie préparera le budget du projet.".into()),
        Segment::new(4_000, 8_000, "Paul contactera le client demain.".into()),
    ];
    let faithful = "Marie préparera le budget du projet et Paul contactera le client demain.";
    let report = MeetingReport {
        language: Language::French,
        titre: "Réunion".into(),
        synthese: format!("{faithful} Alice annoncera une fusion des équipes."),
        decisions: vec![],
        actions: vec![],
        questions: vec![],
        avertissements: vec![],
        sections: vec![SectionNotes {
            first_segment: 0,
            last_segment: 1,
            start_ms: 0,
            end_ms: 8_000,
            titre: "Projet".into(),
            resume: faithful.into(),
            points: vec![],
            decisions: vec![],
            actions: vec![],
            questions: vec![],
        }],
    };
    let rendered = render_synthesis_with_sources(&job, &report);
    let summary = rendered
        .split("## Synthèse\n\n")
        .nth(1)
        .unwrap()
        .split("\n## ")
        .next()
        .unwrap();
    assert!(
        summary.contains(&faithful.replace('.', "&#46;")),
        "synthèse étayée écartée : {summary}"
    );
    assert!(!summary.contains("Alice"), "invention gardée : {summary}");
}

#[test]
fn propositions_inversees_entre_passages_ne_sont_pas_etayees() {
    let mut job = Job::new("réunion.wav".into(), 20_000, 30_000);
    job.segments = vec![
        Segment::new(0, 4_000, "Marie ne valide pas le budget en mai.".into()),
        Segment::new(4_000, 8_000, "Marie valide le contrat en juin.".into()),
    ];
    let faux = "Marie valide le budget en juin et Marie ne valide pas le contrat en mai.";
    let report = MeetingReport {
        language: Language::French,
        titre: "Réunion".into(),
        synthese: faux.into(),
        decisions: vec![],
        actions: vec![],
        questions: vec![],
        avertissements: vec![],
        sections: vec![SectionNotes {
            first_segment: 0,
            last_segment: 1,
            start_ms: 0,
            end_ms: 8_000,
            titre: "Projet".into(),
            resume: faux.into(),
            points: vec![],
            decisions: vec![],
            actions: vec![],
            questions: vec![],
        }],
    };
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(!rendered.contains(faux), "faits croisés admis : {rendered}");

    job.segments = vec![
        Segment::new(0, 4_000, "Marie prépare le budget du projet.".into()),
        Segment::new(4_000, 8_000, "Paul contacte le client demain.".into()),
    ];
    let roles_inverses = "Paul prépare le budget du projet et Marie contacte le client demain.";
    let mut report = report;
    report.synthese = roles_inverses.into();
    report.sections[0].resume = roles_inverses.into();
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        !rendered.contains(roles_inverses),
        "responsables inversés admis : {rendered}"
    );

    job.segments[0].text = "Marie valide. Le budget est reporté.".into();
    job.segments[1].text = "Paul contacte le client.".into();
    let liaison_inventee = "Marie valide le budget et Paul contacte le client.";
    report.synthese = liaison_inventee.into();
    report.sections[0].resume = liaison_inventee.into();
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        !rendered.contains(liaison_inventee),
        "liaison de phrases inventée : {rendered}"
    );

    job.segments[0].text = "Marie valide… Le budget est reporté.".into();
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        !rendered.contains(liaison_inventee),
        "ellipse ignorée : {rendered}"
    );
    job.segments[0].text = "Marie valide » « Le budget est reporté.".into();
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        !rendered.contains(liaison_inventee),
        "citations reliées : {rendered}"
    );
    job.segments[0].text = "Marie valide ’ ‘ Le budget est reporté.".into();
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        !rendered.contains(liaison_inventee),
        "apostrophes reliées : {rendered}"
    );
}

#[test]
fn dates_et_responsables_ne_sont_pas_permutes_dans_un_meme_passage() {
    let mut job = Job::new("réunion.wav".into(), 10_000, 30_000);
    job.segments = vec![Segment::new(
        0,
        8_000,
        "Marie valide le budget le 12 et Paul reporte le contrat le 13.".into(),
    )];
    let faux = "Marie valide le budget le 13 et Paul reporte le contrat le 12.";
    let report = MeetingReport {
        language: Language::French,
        titre: "Réunion".into(),
        synthese: faux.into(),
        decisions: vec![],
        actions: vec![],
        questions: vec![],
        avertissements: vec![],
        sections: vec![SectionNotes {
            first_segment: 0,
            last_segment: 0,
            start_ms: 0,
            end_ms: 8_000,
            titre: "Projet".into(),
            resume: faux.into(),
            points: vec![],
            decisions: vec![],
            actions: vec![],
            questions: vec![],
        }],
    };
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(!rendered.contains(faux), "dates permutées : {rendered}");
    let faux_nouvelle_ligne = "Marie valide le budget le 13 et\nPaul reporte le contrat le 12.";
    let mut report = report;
    report.synthese = faux_nouvelle_ligne.into();
    report.sections[0].resume = faux_nouvelle_ligne.into();
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        !rendered.contains(faux_nouvelle_ligne),
        "conjonction sautée : {rendered}"
    );
    let faux_symbole = "Marie valide le budget le 13 & Paul reporte le contrat le 12.";
    report.synthese = faux_symbole.into();
    report.sections[0].resume = faux_symbole.into();
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        !rendered.contains(faux_symbole),
        "conjonction symbolique : {rendered}"
    );
    let roles = "Paul valide le budget le 12 et Marie reporte le contrat le 13.";
    report.synthese = roles.into();
    report.sections[0].resume = roles.into();
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        !rendered.contains(roles),
        "responsables permutés : {rendered}"
    );
}

#[test]
fn faits_inventes_rejetes() {
    let mut job = Job::new("réunion.wav".into(), 10_000, 30_000);
    job.segments = vec![
        Segment {
            start_ms: 0,
            end_ms: 4_000,
            text: "Nous reportons le lancement en mars.".into(),
            speaker_id: Some("Locuteur 1".into()),
            translated_text: None,
        },
        Segment {
            start_ms: 4_000,
            end_ms: 8_000,
            text: "Marie informe le client vendredi.".into(),
            speaker_id: Some("Locuteur 2".into()),
            translated_text: None,
        },
    ];
    let report = MeetingReport {
        language: Language::French,
        titre: "Réunion du 1er janvier".into(),
        synthese: "Paul signe demain.".into(),
        decisions: vec![TimedText {
            texte: "Le lancement est annulé définitivement".into(),
            start_ms: 0,
        }],
        actions: vec![
            TimedAction {
                responsable: "Marie".into(),
                tache: "Marie informe le client vendredi".into(),
                echeance: "vendredi".into(),
                start_ms: 0,
            },
            TimedAction {
                responsable: "Paul".into(),
                tache: "Paul signe demain".into(),
                echeance: "demain".into(),
                start_ms: 0,
            },
        ],
        questions: vec![],
        avertissements: vec![],
        sections: vec![SectionNotes {
            first_segment: 0,
            last_segment: 1,
            start_ms: 0,
            end_ms: 8_000,
            titre: "Budget final de 1 M€".into(),
            resume: "Le lancement est reporté en mars".into(),
            points: vec!["Marie informe le client vendredi".into()],
            decisions: vec![],
            actions: vec![],
            questions: vec![],
        }],
    };
    let text = render_verified_report(&job, &report);
    assert!(text.contains("Nous reportons le lancement en mars&#46;"));
    assert!(text.contains("Marie informe le client vendredi&#46;"));
    assert!(text.contains("Personne nommée : Marie"));
    assert!(text.contains("Date mentionnée : vendredi"));
    job.speaker_names
        .insert("Locuteur 2".into(), "Thomas".into());
    let renamed = render_verified_report(&job, &report);
    assert!(renamed.contains("Thomas : « Marie informe"));
    assert!(!renamed.contains("Locuteur 2 :"));
    for invention in [
        "1er janvier",
        "Paul signe",
        "demain",
        "annulé définitivement",
        "1 M€",
    ] {
        assert!(
            !text.contains(invention),
            "affirmation non étayée : {invention}"
        );
    }
    let draft = render_synthesis_with_sources(&job, &report);
    assert!(draft.contains("## Synthèse"));
    assert!(draft.contains("Le lancement est reporté en mars"));
    assert!(!draft.contains("Paul signe demain"));
    assert!(!draft.contains("annulé définitivement"));
    assert!(!draft.contains("1 M€"));
    assert!(draft.contains("Décisions proposées - à confirmer"));
    assert!(draft.contains("Passages sources"));
    assert!(draft.contains("[00:00:00"));
    assert!(draft.contains("Nous reportons le lancement en mars&#46;"));
    assert!(!draft.contains("## Décisions\n"));
    let mut grounded = report.clone();
    grounded.synthese =
        "Le lancement est reporté en mars. Marie a reporté le lancement en mars.".into();
    grounded.decisions = vec![
        TimedText {
            texte: "Le lancement est reporté en mars".into(),
            start_ms: 0,
        },
        TimedText {
            texte: "Le lancement en mars".into(),
            start_ms: 0,
        },
    ];
    grounded.questions.push(TimedText {
        texte: "Le lancement est reporté en mars".into(),
        start_ms: 0,
    });
    grounded.actions.retain(|item| item.responsable == "Marie");
    grounded.actions.push(TimedAction {
        responsable: "à confirmer".into(),
        tache: "reporter le lancement".into(),
        echeance: "mars".into(),
        start_ms: 0,
    });
    grounded.synthese.push_str(" Merci à tous.");
    let mut sample = job.clone();
    sample.segments[0].text.push_str(" Merci à tous.");
    let readable = render_synthesis_with_sources(&sample, &grounded);
    let summary = readable
        .split("## Synthèse\n\n")
        .nth(1)
        .unwrap()
        .split("## Décisions")
        .next()
        .unwrap();
    assert!(!summary.contains("Merci à tous"));
    assert!(!readable.contains("| à confirmer | reporter le lancement |"));
    assert!(readable.contains("## Synthèse\n\nLe lancement est reporté en mars&#46;"));
    assert!(!readable.contains("Marie a reporté le lancement"));
    assert!(readable.contains("- [00:00:00] Le lancement est reporté en mars"));
    assert!(!readable.contains("- [00:00:00] Le lancement en mars\n"));
    assert!(!readable
        .contains("## Questions ouvertes\n\n- [00:00:00] Le lancement est reporté en mars"));
    assert!(readable.contains("| Marie | Marie informe le client vendredi | vendredi |"));
    for line in text.lines().filter(|line| line.starts_with("> [")) {
        assert!(
            job.segments
                .iter()
                .any(|s| line.contains(&s.text.replace('.', "&#46;"))),
            "citation absente : {line}"
        );
    }
}

#[test]
fn une_decision_inverse_ou_une_date_changee_ne_sont_pas_etayees() {
    let mut job = Job::new("réunion.wav".into(), 10_000, 30_000);
    job.segments = vec![Segment::new(
        0,
        4_000,
        "Nous reportons le lancement en mars.".into(),
    )];
    let report = MeetingReport {
        language: Language::French,
        titre: "Lancement".into(),
        synthese: "Nous validons le lancement en mars. Le lancement est reporté en avril.".into(),
        decisions: vec![
            TimedText {
                texte: "Nous validons le lancement en mars".into(),
                start_ms: 0,
            },
            TimedText {
                texte: "Nous reportons le lancement en avril".into(),
                start_ms: 0,
            },
            TimedText {
                texte: "Nous reportons le lancement en mars".into(),
                start_ms: 0,
            },
        ],
        actions: vec![],
        questions: vec![],
        avertissements: vec![],
        sections: vec![SectionNotes {
            first_segment: 0,
            last_segment: 0,
            start_ms: 0,
            end_ms: 4_000,
            titre: "Lancement".into(),
            resume: "Nous reportons le lancement en mars".into(),
            points: vec![],
            decisions: vec![],
            actions: vec![],
            questions: vec![],
        }],
    };
    let output = render_synthesis_with_sources(&job, &report);
    assert!(!output.contains("Nous validons le lancement en mars"));
    assert!(!output.contains("Le lancement est reporté en avril"));
    assert!(!output.contains("Nous reportons le lancement en avril"));
    assert!(output.contains("- [00:00:00] Nous reportons le lancement en mars"));
}

#[test]
fn les_propositions_contraires_et_les_dates_deplacees_restent_des_citations_a_verifier() {
    for (source, contresens, exact) in [
        (
            "Nous ne validons pas le lancement ; nous reportons le lancement en mars.",
            "Nous ne reportons pas le lancement en mars",
            "Nous reportons le lancement en mars",
        ),
        (
            "Nous reportons le lancement de mars à avril.",
            "Nous reportons le lancement en mars",
            "Nous reportons le lancement de mars à avril",
        ),
    ] {
        let mut job = Job::new("réunion.wav".into(), 10_000, 30_000);
        job.segments = vec![Segment::new(0, 4_000, source.into())];
        let report = MeetingReport {
            language: Language::French,
            titre: "Lancement".into(),
            synthese: format!("{contresens}. {exact}."),
            decisions: vec![
                TimedText {
                    texte: contresens.into(),
                    start_ms: 0,
                },
                TimedText {
                    texte: exact.into(),
                    start_ms: 0,
                },
            ],
            actions: vec![],
            questions: vec![],
            avertissements: vec![],
            sections: vec![SectionNotes {
                first_segment: 0,
                last_segment: 0,
                start_ms: 0,
                end_ms: 4_000,
                titre: "Lancement".into(),
                resume: exact.into(),
                points: vec![],
                decisions: vec![],
                actions: vec![],
                questions: vec![],
            }],
        };
        let output = render_synthesis_with_sources(&job, &report);
        assert!(
            !output.contains(contresens),
            "contre-sens retenu : {output}"
        );
        assert!(
            output.contains(&format!("- [00:00:00] {exact}")),
            "décision exacte perdue : {output}"
        );
        assert!(
            output.contains(&source.replace('.', "&#46;")),
            "passage source perdu : {output}"
        );
    }
}

#[test]
fn champs_d_un_autre_sujet_dans_la_meme_intervention_restent_a_confirmer() {
    let mut job = Job::new("réunion-fictive.wav".into(), 5_000, 30_000);
    job.source_language = "fr".into();
    job.segments = vec![Segment::new(
        0,
        4_000,
        "Luciole reste le 11 mai. Élodie reprend le tableau à Malik.".into(),
    )];
    let mut report = MeetingReport {
        language: Language::French,
        titre: "Réunion".into(),
        synthese: String::new(),
        decisions: vec![],
        actions: vec![TimedAction {
            responsable: "Élodie".into(),
            tache: "reprend le tableau à Malik".into(),
            echeance: "11 mai".into(),
            start_ms: 0,
        }],
        questions: vec![],
        avertissements: vec![],
        sections: vec![SectionNotes {
            first_segment: 0,
            last_segment: 0,
            start_ms: 0,
            end_ms: 4_000,
            titre: "Tableau".into(),
            resume: String::new(),
            points: vec![],
            decisions: vec![],
            actions: vec![],
            questions: vec![],
        }],
    };
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        rendered.contains("| Élodie | reprend le tableau à Malik | à confirmer |"),
        "{rendered}"
    );
    assert!(!rendered.contains("| Élodie | reprend le tableau à Malik | 11 mai |"));
    assert!(rendered.contains("Luciole reste le 11 mai"));

    job.segments[0].text = "Malik consolide le tableau. Il attend les traces d'Élodie.".into();
    report.actions[0].tache = "consolide le tableau".into();
    report.actions[0].echeance = "non précisé".into();
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        rendered.contains("| à confirmer | consolide le tableau | à confirmer |"),
        "{rendered}"
    );

    job.segments[0].text = "Élodie consolide le tableau le 8 mai.".into();
    report.actions[0].echeance = "8 mai".into();
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        rendered.contains("| Élodie | consolide le tableau | 8 mai |"),
        "{rendered}"
    );

    job.segments[0].text =
        "Je remettrai le tableau à Élodie le 8 mai et je réécrirai le message interne demain."
            .into();
    report.actions[0].tache = "réécrirai le message interne".into();
    report.actions[0].echeance = "8 mai".into();
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        rendered.contains("| à confirmer | réécrirai le message interne | à confirmer |"),
        "{rendered}"
    );

    job.segments[0].text = "Je prépare les légendes pour Élodie.".into();
    report.actions[0].tache = "prépare les légendes".into();
    report.actions[0].echeance = "non précisé".into();
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        rendered.contains("| à confirmer | prépare les légendes | à confirmer |"),
        "{rendered}"
    );

    job.segments[0].text = "Élodie reprend le tableau à Malik.".into();
    report.actions[0].tache = "reprend le tableau".into();
    report.actions[0].responsable = "Malik".into();
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        rendered.contains("| à confirmer | reprend le tableau | à confirmer |"),
        "{rendered}"
    );
    report.actions[0].responsable = "Élodie".into();
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(
        rendered.contains("| Élodie | reprend le tableau | à confirmer |"),
        "{rendered}"
    );
}

#[test]
fn une_rectification_posterieure_est_visible_avant_les_decisions_historiques() {
    // Sortie brute réelle du modèle, conservée avec le corpus inventé : pas de
    // modèle GGUF ni de transcription utilisateur nécessaires à cette régression.
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/reunion-orion-complexe.json")).unwrap();
    let turns = fixture["turns"].as_array().unwrap();
    let mut job = Job::new(
        "reunion-orion-fictive.wav".into(),
        turns.len() as u64 * 10_000,
        30_000,
    );
    job.source_language = "fr".into();
    job.speaker_names = fixture["speakers"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, name)| (id.clone(), name.as_str().unwrap().to_string()))
        .collect();
    for (index, turn) in turns.iter().enumerate() {
        let mut segment = Segment::new(
            index as u64 * 10_000,
            (index as u64 + 1) * 10_000,
            turn["text"].as_str().unwrap().into(),
        );
        segment.speaker_id = Some(turn["speaker"].as_str().unwrap().into());
        job.segments.push(segment);
    }
    let state: parole_core::language::ReportState = serde_json::from_str(include_str!(
        "../../docs/evaluations/orion-2026-09-28/defaut-state-qwen3-4b-instruct-2507-q4_k_m.json"
    ))
    .unwrap();
    let rendered = render_synthesis_with_sources(&job, &state.report.unwrap());
    let correction = rendered
        .find("## Rectifications à vérifier")
        .expect("rectification visible");
    let summary = rendered.find("## Synthèse").unwrap();
    assert!(
        correction < summary,
        "une ancienne date ne doit pas précéder l'alerte"
    );
    assert!(rendered.contains("## Décisions évoquées - historique à vérifier"));
    assert!(rendered.contains("Synthèse suspendue"));
    assert!(!rendered
        .contains("Un pilote interne est prévu le 19 avril sous réserve de validation qualité."));
    let baseline: parole_core::language::ReportState = serde_json::from_str(include_str!(
        "../../docs/evaluations/orion-2026-09-28/defaut-state-baseline.json"
    ))
    .unwrap();
    let baseline_rendered = render_synthesis_with_sources(&job, &baseline.report.unwrap());
    assert!(
        !baseline_rendered.contains("Le 19 avril est proposé pour un pilote interne sous réserve")
    );
    let chronology = rendered
        .split("## Rectifications à vérifier")
        .nth(1)
        .unwrap()
        .split("## Synthèse")
        .next()
        .unwrap();
    assert!(chronology.contains("[00:03:00.000] Marie : « Rectification importante"));
    assert!(chronology
        .contains("[00:03:10.000] Paul : « Nous décidons un nouveau pilote interne le 26 avril"));
    assert!(chronology.contains("nous annulons la date du 19 avril"));
    assert!(
        !chronology.contains("[00:02:00.000]"),
        "l'ancienne décision n'est pas une rectification"
    );
}

#[test]
fn signal_de_rectification_ne_confond_pas_adjectif_et_annulation_actee() {
    let mut job = Job::new("fiction.wav".into(), 10_000, 30_000);
    job.source_language = "en".into();
    job.segments = vec![Segment::new(0, 5_000, "The numbers are correct.".into())];
    let report = MeetingReport {
        language: Language::English,
        titre: String::new(),
        synthese: String::new(),
        decisions: vec![],
        actions: vec![],
        questions: vec![],
        sections: vec![],
        avertissements: vec![],
    };
    let no_revision = render_synthesis_with_sources(&job, &report);
    assert!(!no_revision.contains("## Corrections to review"));
    assert!(no_revision.contains("## Detailed outline"));
    job.segments[0].text = "We do not cancel the pilot.".into();
    let negated = render_synthesis_with_sources(&job, &report);
    assert!(negated.contains("## Corrections to review"));
    assert!(negated.contains("does not establish that it occurred"));
    assert!(negated.contains("We do not cancel the pilot&#46;"));
}

#[test]
fn rectifications_utilisent_le_texte_traduit_quand_le_rapport_est_en_francais() {
    let mut job = Job::new("fiction.wav".into(), 20_000, 30_000);
    job.source_language = "en".into();
    job.target_language = Some("fr".into());
    job.segments = vec![Segment::new(0, 5_000, "We cancel Tuesday.".into())];
    job.segments[0].translated_text = Some("Nous annulons mardi.".into());
    let report = MeetingReport {
        language: Language::French,
        titre: String::new(),
        synthese: String::new(),
        decisions: vec![],
        actions: vec![],
        questions: vec![],
        sections: vec![],
        avertissements: vec![],
    };
    let rendered = render_synthesis_with_sources(&job, &report);
    let corrections = rendered
        .split("## Rectifications à vérifier")
        .nth(1)
        .unwrap()
        .split("## Synthèse")
        .next()
        .unwrap();
    assert!(corrections.contains("Nous annulons mardi&#46;"));
    assert!(!corrections.contains("We cancel Tuesday"));
}

#[test]
fn citations_avec_noms_multilignes_et_html_restent_du_texte() {
    let mut job = Job::new("fiction.wav".into(), 10_000, 30_000);
    job.source_language = "fr".into();
    job.speaker_names
        .insert("S1".into(), "Marie\n## Titre injecté\n<script>".into());
    let mut segment = Segment::new(
        0,
        5_000,
        "Nous annulons mardi <script>alerte</script>.".into(),
    );
    segment.speaker_id = Some("S1".into());
    job.segments.push(segment);
    let report = MeetingReport {
        language: Language::French,
        titre: String::new(),
        synthese: String::new(),
        decisions: vec![],
        actions: vec![],
        questions: vec![],
        sections: vec![],
        avertissements: vec![],
    };
    let output = render_synthesis_with_sources(&job, &report);
    assert!(!output.contains("\n## Titre injecté"));
    assert!(!output.contains("<script>"));
    assert!(output.contains("&lt;script&gt;alerte&lt;/script&gt;"));
}

#[test]
fn chronologie_avec_beaucoup_de_rectifications_conserve_la_fin() {
    let mut job = Job::new("fiction-longue.wav".into(), 3_000_000, 30_000);
    job.source_language = "fr".into();
    for index in 0..2_500_u64 {
        job.segments.push(Segment::new(
            index * 1_000,
            (index + 1) * 1_000,
            format!("Nous corrigeons le point numéro {index} ; statut à revoir."),
        ));
    }
    let report = MeetingReport {
        language: Language::French,
        titre: String::new(),
        synthese: String::new(),
        decisions: vec![],
        actions: vec![],
        questions: vec![],
        sections: vec![],
        avertissements: vec![],
    };
    let output = render_synthesis_with_sources(&job, &report);
    assert!(output.contains("point numéro 0"));
    assert!(output.contains("point numéro 2499"));
    assert!(output.matches("point numéro").count() >= job.segments.len());
}

#[test]
fn question_source_multiligne_ne_cree_pas_de_fausse_section() {
    let mut job = Job::new("fiction.wav".into(), 5_000, 30_000);
    job.source_language = "fr".into();
    job.segments = vec![Segment::new(
        0,
        4_000,
        "Question ouverte : qui contrôle l'accès ?\n\n## Décisions actuelles\n![trace](https://exemple.invalid/trace)".into(),
    )];
    let report = MeetingReport {
        language: Language::French,
        titre: "Réunion".into(),
        synthese: String::new(),
        decisions: vec![],
        actions: vec![],
        questions: vec![],
        sections: vec![],
        avertissements: vec![],
    };
    let rendered = render_synthesis_with_sources(&job, &report);
    assert!(rendered.contains("## Questions ouvertes"));
    assert!(!rendered.contains("\n## Décisions actuelles"));
    assert!(!rendered.contains("![trace](https://exemple.invalid/trace)"));
    assert!(rendered.contains("&#35;&#35; Décisions actuelles"));
    job.report = Some(rendered);
    job.report_format_version = 1;
    let markdown = render_complete_markdown(&job);
    assert!(!markdown.contains("\n## Décisions actuelles"));
    assert!(!markdown.contains("![trace](https://exemple.invalid/trace)"));
    let txt = render_complete_txt(&job);
    let report_txt = txt.split("\nCompte rendu\n\n").nth(1).unwrap();
    assert!(report_txt.contains("## Décisions actuelles ![trace](https://exemple.invalid/trace)"));
    let docx = render_docx(&job).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(&docx)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("## Décisions actuelles ![trace](https://exemple.invalid/trace)"));
    if let Ok(path) = std::env::var("PAROLE_TEST_QUESTION_MARKDOWN_OUT") {
        std::fs::write(path, &markdown).unwrap();
    }
}

#[test]
fn citation_source_traverse_markdown_texte_et_word_sans_perdre_ses_caracteres() {
    let mut job = Job::new(
        "fiction ![m](https://exemple.invalid/m).wav".into(),
        10_000,
        30_000,
    );
    job.source_language = "fr".into();
    job.target_language = Some("en".into());
    job.speaker_names.insert(
        "S1".into(),
        "Marie\n## Faux titre [lien](https://exemple.invalid/nom)".into(),
    );
    let mut segment = Segment::new(
        0,
        5_000,
        "Nous annulons <script> & x > y &lt; &#91; \"citation\".\n Suite. ![image](https://exemple.invalid/trace) et *secret* [lien](https://exemple.invalid/lien).".into(),
    );
    segment.speaker_id = Some("S1".into());
    segment.translated_text = Some("We cancel [link](https://exemple.invalid/translated).".into());
    job.segments.push(segment);
    let report = MeetingReport {
        language: Language::French,
        titre: String::new(),
        synthese: String::new(),
        decisions: vec![],
        actions: vec![],
        questions: vec![],
        sections: vec![],
        avertissements: vec![],
    };
    job.report = Some(render_synthesis_with_sources(&job, &report));
    job.report_format_version = 1;
    job.report
        .as_mut()
        .unwrap()
        .push_str("\n![sortie](https://exemple.invalid/gen)\n");
    let markdown = render_complete_markdown(&job);
    assert!(
        markdown.contains("&lt;script&gt; &amp; x &gt; y &amp;lt; &amp;&#35;91; \"citation\"&#46;")
    );
    assert!(!markdown.contains("\n## Faux titre"));
    assert!(!markdown.contains("![image](https://exemple.invalid/trace)"));
    assert!(!markdown.contains("![sortie](https://exemple.invalid/gen)"));
    assert!(!markdown.contains("[link](https://exemple.invalid/translated)"));
    assert!(!markdown.contains("![m](https://exemple.invalid/m)"));
    assert!(job.report.as_ref().unwrap().contains("&#91;image&#93;"));
    if let Ok(path) = std::env::var("PAROLE_TEST_MARKDOWN_OUT") {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        file.write_all(markdown.as_bytes()).unwrap();
    }
    let plain = render_complete_txt(&job);
    let quote = plain.lines().find(|line| line.starts_with("> [")).unwrap();
    assert!(quote.contains("<script> & x > y &lt; &#91; \"citation\"."));
    assert!(quote.contains("![image](https://exemple.invalid/trace) et *secret* [lien]"));
    assert!(!quote.contains("&amp; x"));
    let bytes = render_docx(&job).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("&lt;script&gt; &amp; x &gt; y &amp;lt; &amp;#91; &quot;citation&quot;."));
    assert!(xml.contains("![image](https://exemple.invalid/trace) et *secret* [lien]"));
    assert!(!xml.contains("&amp;lt;script&amp;gt;"));
}

#[test]
#[ignore = "Rejoue sans modèle quatre états fictifs conservés ; PAROLE_REPLAY_OUT requis"]
fn rejouer_les_quatre_sorties_fictives_avec_le_nouveau_rendu() {
    let parent = std::path::PathBuf::from(
        std::env::var("PAROLE_REPLAY_OUT").expect("PAROLE_REPLAY_OUT requis"),
    );
    std::fs::create_dir_all(&parent).unwrap();
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output = parent.join(format!("orion-rejeu-{run}-{}", std::process::id()));
    std::fs::create_dir(&output).expect("nouveau dossier de relecture requis");
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/reunion-orion-complexe.json")).unwrap();
    let turns = fixture["turns"].as_array().unwrap();
    let mut job = Job::new(
        "reunion-orion-fictive.wav".into(),
        turns.len() as u64 * 10_000,
        30_000,
    );
    job.source_language = "fr".into();
    job.speaker_names = fixture["speakers"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, name)| (id.clone(), name.as_str().unwrap().to_string()))
        .collect();
    for (index, turn) in turns.iter().enumerate() {
        let mut segment = Segment::new(
            index as u64 * 10_000,
            (index as u64 + 1) * 10_000,
            turn["text"].as_str().unwrap().into(),
        );
        segment.speaker_id = Some(turn["speaker"].as_str().unwrap().into());
        job.segments.push(segment);
    }
    for (name, state) in [
        ("defaut-baseline", include_str!("../../docs/evaluations/orion-2026-09-28/defaut-state-baseline.json")),
        ("defaut-qwen3", include_str!("../../docs/evaluations/orion-2026-09-28/defaut-state-qwen3-4b-instruct-2507-q4_k_m.json")),
        ("stress-baseline", include_str!("../../docs/evaluations/orion-2026-09-28/stress-state-baseline.json")),
        ("stress-qwen3", include_str!("../../docs/evaluations/orion-2026-09-28/stress-state-qwen3-4b-instruct-2507-q4_k_m.json")),
    ] {
        let state: parole_core::language::ReportState = serde_json::from_str(state).unwrap();
        let rendered = render_synthesis_with_sources(&job, &state.report.unwrap());
        assert!(rendered.contains("## Rectifications à vérifier"));
        assert!(rendered.contains("le 26 avril"));
        let path = output.join(format!("{name}.md"));
        std::fs::write(&path, rendered).unwrap();
        eprintln!("{name}: {}", path.display());
    }
}
