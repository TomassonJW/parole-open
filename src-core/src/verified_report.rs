//! Compte rendu fondé sur des citations exactes.
use crate::language::{markdown_inline_literal, Language, MeetingReport, SectionNotes, TimedText};
use crate::{format_timestamp, Job, Segment};
use std::collections::{BTreeMap, BTreeSet};

fn words(text: &str) -> BTreeSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 4)
        .map(str::to_string)
        .collect()
}

fn quote_text<'a>(job: &Job, report: &MeetingReport, segment: &'a Segment) -> &'a str {
    if job.target_language.as_deref() == Some(report.language.code()) {
        segment.translated_text.as_deref().unwrap_or(&segment.text)
    } else {
        &segment.text
    }
}

fn normalize_words(text: &str) -> String {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn mentioned(quote: &str, value: &str) -> bool {
    let phrase = normalize_words(value);
    !phrase.is_empty() && format!(" {} ", normalize_words(quote)).contains(&format!(" {phrase} "))
}

fn mentioned_in_action_clause(quote: &str, task: &str, value: &str, responsible: bool) -> bool {
    // Une même phrase peut contenir deux tâches distinctes (« ... et je ... »).
    // Garder ce garde conservateur : il vérifie la proximité, pas l'attribution sémantique.
    let split_subjects = [
        (" et je ", "; je "),
        (" et j'", "; j'"),
        (" et il ", "; il "),
        (" et elle ", "; elle "),
        (" et nous ", "; nous "),
        (" et on ", "; on "),
        (" et ils ", "; ils "),
        (" et elles ", "; elles "),
        (" et tu ", "; tu "),
        (" et vous ", "; vous "),
    ];
    let clauses = split_subjects
        .iter()
        .fold(quote.to_string(), |text, (before, after)| {
            text.replace(before, after)
        });
    clauses
        .split([';', '.', '!', '?'])
        .map(str::trim)
        .filter(|clause| !clause.is_empty())
        .any(|clause| {
            let normalized = format!(" {} ", normalize_words(clause));
            let name = normalize_words(value);
            let recipient = responsible
                && ["pour", "à"]
                    .iter()
                    .any(|preposition| normalized.contains(&format!(" {preposition} {name} ")));
            supported(task, clause) && mentioned(clause, value) && !recipient
        })
}

fn quote_inline(text: &str) -> String {
    markdown_inline_literal(text)
}

fn format_quote(job: &Job, report: &MeetingReport, segment: &Segment) -> String {
    let time = format_timestamp(segment.start_ms).replace(',', ".");
    let speaker = segment
        .speaker_id
        .as_deref()
        .and_then(|id| job.speaker_names.get(id).map(String::as_str).or(Some(id)))
        .map(quote_inline)
        .unwrap_or_else(|| "Voix non attribuée".into());
    let text = quote_inline(quote_text(job, report, segment));
    format!("> [{time}] {speaker} : « {text} »\n")
}

/// Restitue une seule couche du format textuel v1 après avoir identifié,
/// par la version du travail, qu'il ne s'agit pas d'un ancien rapport brut.
pub(crate) fn decode_encoded_report_text(markdown: &str) -> String {
    let mut visible = markdown.to_string();
    for (encoded, literal) in [
        ("&#33;", "!"),
        ("&#58;", ":"),
        ("&#46;", "."),
        ("&#64;", "@"),
        ("&#91;", "["),
        ("&#93;", "]"),
        ("&#40;", "("),
        ("&#41;", ")"),
        ("&#42;", "*"),
        ("&#95;", "_"),
        ("&#96;", "`"),
        ("&#126;", "~"),
        ("&#92;", "\\"),
        ("&#124;", "|"),
        ("&#35;", "#"),
        ("&#43;", "+"),
        ("&#45;", "-"),
        ("&#123;", "{"),
        ("&#125;", "}"),
    ] {
        visible = visible.replace(encoded, literal);
    }
    visible
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn match_quote<'a>(
    job: &'a Job,
    report: &MeetingReport,
    start: u64,
    hint: &str,
) -> Option<&'a Segment> {
    let section = report.sections.iter().find(|s| s.start_ms == start)?;
    let terms = words(hint);
    if terms.is_empty() {
        return None;
    }
    let range = job
        .segments
        .get(section.first_segment..=section.last_segment)?;
    range
        .iter()
        .filter(|s| !s.text.trim().is_empty())
        .max_by_key(|s| {
            terms
                .intersection(&words(quote_text(job, report, s)))
                .count()
        })
        .filter(|s| {
            terms
                .intersection(&words(quote_text(job, report, s)))
                .count()
                >= 2
        })
}

fn section_quote<'a>(
    job: &'a Job,
    report: &MeetingReport,
    section: &SectionNotes,
) -> Option<&'a Segment> {
    match_quote(job, report, section.start_ms, &section.resume)
        .or_else(|| job.segments.get(section.first_segment))
}

/// N'affiche que des citations réellement présentes dans la transcription.
pub fn render_verified_report(job: &Job, report: &MeetingReport) -> String {
    let mut out = format!("# {}\n\n", quote_inline(&job.media_name));
    if report.language == Language::French {
        out.push_str("Citations extraites du texte reconnu ; vérifier les voix et les propos avant diffusion.\n\n## Synthèse\n\n");
    } else {
        out.push_str("Direct transcript quotations; check recognition and speaker attribution before sharing.\n\n## Summary\n\n");
    }
    for section in report.sections.iter().take(3) {
        if let Some(s) = section_quote(job, report, section) {
            out.push_str(&format_quote(job, report, s));
        }
    }
    out.push_str(if report.language == Language::French {
        "\n## Sujets abordés\n\n"
    } else {
        "\n## Main topics\n\n"
    });
    for section in &report.sections {
        if let Some(s) = section_quote(job, report, section) {
            out.push_str(&format_quote(job, report, s));
        }
    }
    out.push_str(if report.language == Language::French {
        "\n## Décisions évoquées (à confirmer)\n\n"
    } else {
        "\n## Possible decisions (verify)\n\n"
    });
    let mut used = BTreeSet::new();
    for item in &report.decisions {
        if let Some(s) = match_quote(job, report, item.start_ms, &item.texte) {
            if used.insert(s.start_ms) {
                out.push_str(&format_quote(job, report, s));
            }
        }
    }
    out.push_str(if report.language == Language::French {
        "\n## Actions évoquées (à confirmer)\n\n"
    } else {
        "\n## Possible action items (verify)\n\n"
    });
    used.clear();
    for item in &report.actions {
        if let Some(s) = match_quote(job, report, item.start_ms, &item.tache) {
            if used.insert(s.start_ms) {
                out.push_str(&format_quote(job, report, s));
                let quote = quote_text(job, report, s);
                if mentioned(quote, &item.responsable)
                    && item.responsable != "non précisé"
                    && item.responsable != "not specified"
                {
                    let label = if report.language == Language::French {
                        "Personne nommée"
                    } else {
                        "Person mentioned"
                    };
                    out.push_str(&format!(
                        "  - {label} : {}\n",
                        quote_inline(&item.responsable)
                    ));
                }
                if mentioned(quote, &item.echeance)
                    && item.echeance != "non précisé"
                    && item.echeance != "not specified"
                {
                    let label = if report.language == Language::French {
                        "Date mentionnée"
                    } else {
                        "Date mentioned"
                    };
                    out.push_str(&format!("  - {label} : {}\n", quote_inline(&item.echeance)));
                }
            }
        }
    }
    out.push_str(if report.language == Language::French {
        "\n## Questions ouvertes\n\n"
    } else {
        "\n## Open questions\n\n"
    });
    used.clear();
    for item in &report.questions {
        if let Some(s) = match_quote(job, report, item.start_ms, &item.texte) {
            if quote_text(job, report, s).contains('?') && used.insert(s.start_ms) {
                out.push_str(&format_quote(job, report, s));
            }
        }
    }
    out.push_str(if report.language == Language::French {
        "\n## Faits et références\n\n"
    } else {
        "\n## Important facts and references\n\n"
    });
    used.clear();
    for section in &report.sections {
        for point in &section.points {
            if let Some(s) = match_quote(job, report, section.start_ms, point) {
                if used.insert(s.start_ms) {
                    out.push_str(&format_quote(job, report, s));
                }
            }
        }
    }
    out.push_str(if report.language == Language::French {
        "\n## Risques ou blocages évoqués\n\n"
    } else {
        "\n## Risks or blockers mentioned\n\n"
    });
    used.clear();
    for s in &job.segments {
        let text = quote_text(job, report, s).to_lowercase();
        if [
            "risque",
            "bloqu",
            "retard",
            "problème",
            "risk",
            "block",
            "delay",
            "problem",
        ]
        .iter()
        .any(|term| text.contains(term))
            && used.insert(s.start_ms)
        {
            out.push_str(&format_quote(job, report, s));
        }
    }
    out
}

/// Contrôle conservateur de proximité lexicale : il élimine les inventions évidentes,
/// mais ne prouve jamais à lui seul la vérité d'une paraphrase.
fn supported(candidate: &str, source: &str) -> bool {
    // Une négation d'une proposition ne peut pas étayer la proposition suivante.
    let clauses = source
        .split([';', '.', '!', '?'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if clauses.len() > 1 {
        return clauses.iter().any(|part| supported(candidate, part));
    }
    const STOP: &[&str] = &[
        "avec", "dans", "pour", "nous", "vous", "cette", "cela", "dont", "sont", "avait", "avoir",
        "fait", "être", "from", "were", "about", "that", "this", "there", "will", "they", "them",
    ];
    let terms: Vec<_> = words(candidate)
        .into_iter()
        .filter(|term| !STOP.contains(&term.as_str()))
        .collect();
    if terms.len() < 2 {
        return false;
    }
    let source_words = words(source);
    // Un verbe de décision contraire ne devient pas vrai par simple recouvrement du sujet.
    const DECISIVE_ROOTS: &[&str] = &[
        "valid", "report", "annul", "refus", "accept", "approuv", "suspend", "confirm", "mainten",
        "décid", "decid", "postpon", "cancel", "reject", "approv", "delay",
    ];
    if DECISIVE_ROOTS.iter().any(|root| {
        terms.iter().any(|term| term.starts_with(root))
            && !source_words.iter().any(|word| word.starts_with(root))
    }) {
        return false;
    }
    let is_negative = |text: &str| {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| {
                ["pas", "jamais", "aucun", "sans", "not", "never", "without"].contains(&word)
            })
    };
    if is_negative(candidate) != is_negative(source) {
        return false;
    }
    // Les mois, y compris « mai », sont des dates et non des synonymes.
    const MONTHS: &[&str] = &[
        "janvier",
        "février",
        "fevrier",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "aout",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
        "decembre",
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    let distinct_months = source
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| MONTHS.contains(word))
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if distinct_months.len() > 1 {
        // Deux dates peuvent désigner l'ancienne et la nouvelle échéance : citer
        // l'énoncé entier plutôt que deviner à laquelle renvoie une paraphrase.
        let tokens = |text: &str| {
            text.to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        if tokens(candidate) != tokens(source) {
            return false;
        }
    }
    if candidate
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| {
            MONTHS.contains(&word)
                && !source
                    .to_lowercase()
                    .split(|c: char| !c.is_alphanumeric())
                    .any(|original| original == word)
        })
    {
        return false;
    }
    // Un nom propre ne peut pas être emprunté à un autre passage pour inventer son rôle.
    for original in candidate.split(|c: char| !c.is_alphanumeric()) {
        let capitalized = original.chars().next().is_some_and(char::is_uppercase);
        if capitalized
            && original.chars().count() >= 3
            && ![
                "Le", "La", "Les", "Un", "Une", "Des", "Nous", "On", "Ce", "Cette", "Il", "Elle",
                "Au", "De", "Du", "Dans", "Et", "The", "We", "For",
            ]
            .contains(&original)
            && !source
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .any(|word| word == original.to_lowercase())
        {
            return false;
        }
    }
    // Chiffres et négations sont des faits sensibles : jamais de correspondance approximative.
    for term in candidate
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
    {
        let requires_exact_match = term.chars().any(|c| c.is_ascii_digit())
            || [
                "pas",
                "jamais",
                "aucun",
                "annulé",
                "annule",
                "not",
                "never",
                "cancelled",
            ]
            .contains(&term);
        if requires_exact_match
            && !source
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .any(|word| word == term)
        {
            return false;
        }
    }
    let matches = terms
        .iter()
        .filter(|term| {
            source_words.iter().any(|original| {
                original == *term
                    || (term.chars().count() >= 6
                        && original.chars().count() >= 6
                        && original.chars().take(5).eq(term.chars().take(5)))
            })
        })
        .count();
    matches >= 2 && matches * 5 >= terms.len() * 3
}

fn decision_signal(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "décid", "decid", "report", "valid", "approuv", "approved", "postpon", "annul", "choisi",
        "accept", "acté", "convenu",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

fn question_signal(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains('?')
        || [
            "question",
            "point ouvert",
            "open issue",
            "whether",
            "faut-il",
            "si nous",
            "à discuter",
        ]
        .iter()
        .any(|word| lower.contains(word))
}

fn open_question_signal(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "point ouvert",
        "question ouverte",
        "reste à décider",
        "sans réponse",
        "open question",
        "unresolved",
        "undecided",
        "ne savons pas si",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

fn split_conjunctions(sentence: &str, language: Language) -> Vec<&str> {
    let conjunction = if language == Language::French {
        "et"
    } else {
        "and"
    };
    let mut clauses = Vec::new();
    let mut start = 0;
    for (index, ch) in sentence.char_indices() {
        if matches!(ch, '&' | '+' | '/' | '|') {
            clauses.push(sentence[start..index].trim());
            start = index + ch.len_utf8();
            continue;
        }
        if !sentence[index..]
            .get(..conjunction.len())
            .is_some_and(|word| word.eq_ignore_ascii_case(conjunction))
        {
            continue;
        }
        let end = index + conjunction.len();
        if sentence[..index]
            .chars()
            .next_back()
            .is_some_and(char::is_alphabetic)
            || sentence[end..]
                .chars()
                .next()
                .is_some_and(char::is_alphabetic)
        {
            continue;
        }
        clauses.push(sentence[start..index].trim());
        start = end;
    }
    clauses.push(sentence[start..].trim());
    clauses
}

fn literal_clause_in_source(source: &str, clause: &str) -> bool {
    // Ne jamais effacer la ponctuation entre deux affirmations : la liste
    // des signes de séparation ne peut pas être exhaustive (guillemets,
    // ellipse Unicode, etc.). Seuls les espaces peuvent être normalisés.
    let source = source
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let clause = clause
        .trim()
        .trim_end_matches(['.', '!', '?'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if clause.is_empty() {
        return false;
    }
    source.match_indices(&clause).any(|(start, _)| {
        let end = start + clause.len();
        !source[..start]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
            && !source[end..]
                .chars()
                .next()
                .is_some_and(char::is_alphanumeric)
    })
}

fn supported_clause_sequence(clauses: &[&str], sources: &[&str]) -> bool {
    // Une phrase composée n'emprunte jamais le contrôle lexical global :
    // il laisserait permuter deux dates ou deux personnes dans la même source.
    if !(2..=3).contains(&clauses.len()) {
        return false;
    }
    clauses.iter().enumerate().all(|(index, clause)| {
        if index > 0 {
            let subject = clause.split_whitespace().next().unwrap_or("");
            if !subject.chars().next().is_some_and(char::is_uppercase)
                && ![
                    "nous", "on", "il", "elle", "ils", "elles", "we", "he", "she", "they",
                ]
                .contains(&subject.to_lowercase().as_str())
            {
                return false;
            }
        }
        sources
            .iter()
            .any(|source| supported(clause, source) && literal_clause_in_source(source, clause))
    })
}

fn supported_sentences<'a>(
    draft: &str,
    passages: impl IntoIterator<Item = &'a Segment>,
    job: &Job,
    report: &MeetingReport,
) -> String {
    let sources = passages
        .into_iter()
        .map(|segment| quote_text(job, report, segment))
        .collect::<Vec<_>>();
    draft
        .split_inclusive(['.', '!', '?'])
        .map(str::trim)
        .filter(|sentence| {
            let lower = sentence.to_lowercase();
            let clauses = split_conjunctions(sentence, report.language);
            let verified = if clauses.len() > 1 {
                supported_clause_sequence(&clauses, &sources)
            } else {
                sources.iter().any(|source| supported(sentence, source))
            };
            !lower.starts_with("merci ")
                && !lower.starts_with("thanks ")
                && !lower.starts_with("thank you")
                && verified
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn source_for(job: &Job, report: &MeetingReport, section: &SectionNotes) -> String {
    job.segments
        .get(section.first_segment..=section.last_segment)
        .unwrap_or(&[])
        .iter()
        .map(|s| quote_text(job, report, s))
        .collect::<Vec<_>>()
        .join(" ")
}

fn explicit_revision(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "annul",
        "rectifi",
        "corrig",
        "rempla",
        "cancel",
        "correction",
        "corrected",
        "correcting",
        "replac",
        "withdraw",
    ]
    .iter()
    .any(|cue| lower.contains(cue))
}

/// Repère seulement les paroles qui annoncent une rectification et, si elle
/// contient une décision explicite, la prise de parole immédiatement suivante.
/// Aucune conclusion automatique sur l'état final n'est déduite de ces mots.
fn revision_excerpts(job: &Job, report: &MeetingReport) -> BTreeSet<usize> {
    let mut selected = BTreeSet::new();
    for (index, segment) in job.segments.iter().enumerate() {
        if !explicit_revision(quote_text(job, report, segment)) {
            continue;
        }
        selected.insert(index);
        if let Some(next) = job.segments.get(index + 1) {
            if decision_signal(quote_text(job, report, next)) {
                selected.insert(index + 1);
            }
        }
    }
    selected
}

/// Résumé rédigé et sourcé. Les filtres écartent les inventions flagrantes ;
/// l'utilisateur doit relire les formulations et confirmer toute décision/action.
pub fn render_synthesis_with_sources(job: &Job, report: &MeetingReport) -> String {
    use crate::language::render_report_markdown_with_presentation;
    let mut safe = report.clone();
    let corpus = job
        .segments
        .iter()
        .map(|s| quote_text(job, report, s))
        .collect::<Vec<_>>()
        .join(" ");
    if !supported(&safe.titre, &corpus) {
        safe.titre = if safe.language == Language::French {
            "Compte rendu de réunion".into()
        } else {
            "Meeting report".into()
        };
    }
    safe.sections = report
        .sections
        .iter()
        .enumerate()
        .map(|(index, section)| {
            let mut copy = section.clone();
            let source = source_for(job, report, section);
            if !supported(&copy.titre, &source) {
                copy.titre = format!("Passage {}", index + 1);
            }
            copy.resume = supported_sentences(
                &section.resume,
                job.segments
                    .get(section.first_segment..=section.last_segment)
                    .unwrap_or(&[])
                    .iter(),
                job,
                report,
            );
            copy.points.retain(|point| supported(point, &source));
            copy.decisions.retain(|point| {
                supported(point, &source) && decision_signal(point) && decision_signal(&source)
            });
            copy.actions
                .retain(|point| supported(&point.tache, &source));
            copy.questions.retain(|point| {
                supported(point, &source) && question_signal(point) && question_signal(&source)
            });
            copy
        })
        .collect();
    safe.synthese = supported_sentences(&report.synthese, job.segments.iter(), job, report);
    if safe.synthese.is_empty() {
        safe.synthese = safe
            .sections
            .iter()
            .filter(|s| !s.resume.is_empty())
            .take(5)
            .map(|s| s.resume.as_str())
            .collect::<Vec<_>>()
            .join(" ");
    }
    if safe.synthese.is_empty() {
        safe.synthese = if safe.language == Language::French {
            "Synthèse non étayée : consulter les passages sources.".into()
        } else {
            "Summary not supported: consult the source excerpts.".into()
        };
    }
    safe.decisions = report
        .decisions
        .iter()
        .filter_map(|item| {
            if !decision_signal(&item.texte) {
                return None;
            }
            let segment = match_quote(job, report, item.start_ms, &item.texte)?;
            let quote = quote_text(job, report, segment);
            if !decision_signal(quote) || !supported(&item.texte, quote) {
                return None;
            }
            Some(TimedText {
                texte: item.texte.clone(),
                start_ms: segment.start_ms,
            })
        })
        .collect();
    safe.actions = report
        .actions
        .iter()
        .filter_map(|item| {
            let segment = match_quote(job, report, item.start_ms, &item.tache)?;
            let quote = quote_text(job, report, segment);
            if !supported(&item.tache, quote) {
                return None;
            }
            let mut copy = item.clone();
            copy.start_ms = segment.start_ms;
            if !mentioned_in_action_clause(quote, &copy.tache, &copy.responsable, true) {
                copy.responsable = "à confirmer".into();
            }
            if !mentioned_in_action_clause(quote, &copy.tache, &copy.echeance, false) {
                copy.echeance = "à confirmer".into();
            }
            Some(copy)
        })
        .collect();
    safe.questions = report
        .questions
        .iter()
        .filter_map(|item| {
            if !question_signal(&item.texte) {
                return None;
            }
            let segment = match_quote(job, report, item.start_ms, &item.texte)?;
            let quote = quote_text(job, report, segment);
            if !open_question_signal(quote) || !supported(&item.texte, quote) {
                return None;
            }
            Some(TimedText {
                texte: item.texte.clone(),
                start_ms: segment.start_ms,
            })
        })
        .collect();
    if safe.questions.is_empty() {
        safe.questions = job
            .segments
            .iter()
            .filter(|segment| open_question_signal(quote_text(job, report, segment)))
            .take(12)
            .map(|segment| TimedText {
                texte: quote_text(job, report, segment).to_string(),
                start_ms: segment.start_ms,
            })
            .collect();
    }
    let classified_events: BTreeSet<_> = safe
        .decisions
        .iter()
        .map(|item| item.start_ms)
        .chain(safe.questions.iter().map(|item| item.start_ms))
        .collect();
    safe.actions.retain(|item| {
        item.responsable != "à confirmer" || !classified_events.contains(&item.start_ms)
    });
    let revisions = revision_excerpts(job, report);
    let mut block = String::new();
    if !revisions.is_empty() {
        // Une ancienne décision peut rester au présent dans les notes du modèle.
        // Tant que les remplacements ne sont pas établis, ne pas publier ces
        // paragraphes comme synthèse ou déroulé courant.
        safe.titre = if safe.language == Language::French {
            "Compte rendu à vérifier".into()
        } else {
            "Meeting report for review".into()
        };
        safe.synthese = if safe.language == Language::French {
            "Synthèse suspendue : des rectifications possibles rendent le dernier état des décisions incertain. Lire les paroles horodatées ci-dessus ; les listes ci-dessous sont historiques et ne prouvent pas ce qui reste en vigueur.".into()
        } else {
            "Summary withheld: possible corrections make the final state of decisions uncertain. Review the timestamped excerpts above; the lists below are historical and do not establish what is still in force.".into()
        };
        safe.sections.clear();
        block = if safe.language == Language::French {
            "## Rectifications à vérifier\n\nCertaines paroles évoquent une correction ou une annulation, sans garantir qu'elle a eu lieu. Les décisions et actions citées plus bas sont historiques, pas nécessairement encore valables. Extraits dans l'ordre, sans déduction automatique du dernier état :\n\n".to_string()
        } else {
            "## Corrections to review\n\nSome passages mention a possible correction or cancellation; this does not establish that it occurred. Decisions and actions below are historical, not necessarily still valid. Excerpts in order, without inferring a final state:\n\n".to_string()
        };
        for index in &revisions {
            block.push_str(&format_quote(job, report, &job.segments[*index]));
        }
        block.push('\n');
    }
    let (decisions_title, actions_title, questions_title) =
        match (safe.language, revisions.is_empty()) {
            (Language::French, true) => (
                "Décisions proposées - à confirmer",
                "Actions proposées - à confirmer",
                "Questions ouvertes",
            ),
            (Language::French, false) => (
                "Décisions évoquées - historique à vérifier",
                "Actions évoquées - historique à vérifier",
                "Questions évoquées - état à vérifier",
            ),
            (_, true) => (
                "Proposed decisions - verify",
                "Proposed actions - verify",
                "Open questions",
            ),
            (_, false) => (
                "Decisions discussed - historical, review status",
                "Actions discussed - historical, review status",
                "Questions discussed - review status",
            ),
        };
    let mut out = render_report_markdown_with_presentation(
        &safe,
        &block,
        Some(decisions_title),
        Some(actions_title),
        Some(questions_title),
        !revisions.is_empty(),
    );
    if safe.language == Language::French {
        out.push_str("\n\n## Passages sources\n\nLes citations ci-dessous sont exactes dans la transcription, qui peut elle-même comporter des erreurs. Le rapprochement lexical ne prouve pas la justesse du résumé : relisez avant toute diffusion.\n\n");
    } else {
        out.push_str("\n\n## Source excerpts\n\nQuotes come from the transcript, which may contain recognition errors. Lexical matching does not prove the summary is correct; review before sharing.\n\n");
    }
    let mut cited = BTreeSet::new();
    for section in &safe.sections {
        if let Some(segment) = section_quote(job, report, section) {
            cited.insert(segment.start_ms);
        }
    }
    cited.extend(safe.decisions.iter().map(|item| item.start_ms));
    cited.extend(safe.actions.iter().map(|item| item.start_ms));
    cited.extend(safe.questions.iter().map(|item| item.start_ms));
    cited.extend(revisions.iter().map(|index| job.segments[*index].start_ms));
    // Conserver le premier segment à chaque horodatage, comme l'ancien `find`,
    // sans refaire une traversée complète pour chaque rectification citée.
    let mut by_time = BTreeMap::new();
    for segment in &job.segments {
        by_time.entry(segment.start_ms).or_insert(segment);
    }
    for start in cited {
        if let Some(segment) = by_time.get(&start) {
            out.push_str(&format_quote(job, report, segment));
        }
    }
    out
}
