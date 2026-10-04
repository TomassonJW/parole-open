use parole_core::topic_questions::TopicQuestion;
use tokenizers::Tokenizer;

pub(crate) struct Encoded {
    pub(crate) input_ids: Vec<i64>,
    pub(crate) attention_mask: Vec<i64>,
}
pub(crate) fn encode(tokenizer: &Tokenizer, question: &TopicQuestion) -> Result<Encoded, String> {
    let input = question
        .model_input
        .as_ref()
        .ok_or("Aucun sujet n'est demandé.")?;
    if input.text.trim().is_empty()
        || input.text.len() > 65_536
        || input.labels.is_empty()
        || input.labels.len() > 25
        || input.labels.len() != question.candidates.len()
        || input
            .labels
            .iter()
            .zip(&question.candidates)
            .any(|(label, candidate)| {
                label.trim().is_empty() || label.len() > 4096 || label != &candidate.label
            })
    {
        return Err(
            "La liste des sujets est incomplète ou ne correspond pas à la question.".into(),
        );
    }
    if tokenizer.get_added_tokens_decoder().values().any(|token| {
        token.special
            && (input.text.contains(&token.content)
                || input
                    .labels
                    .iter()
                    .any(|label| label.contains(&token.content)))
    }) {
        return Err("Le texte ou un sujet contient un marqueur réservé au modèle.".into());
    }
    let prompt = format!(
        "{}<<SEP>>{}",
        input
            .labels
            .iter()
            .map(|s| format!("<<LABEL>>{s}"))
            .collect::<String>(),
        input.text
    );
    let mut complete = tokenizer.clone();
    complete
        .with_truncation(None)
        .map_err(|_| "Le tokenizer ne peut pas être utilisé sans troncature.")?;
    complete.with_padding(None);
    let item = complete
        .encode(prompt, true)
        .map_err(|_| "Le passage ne peut pas être préparé pour le modèle.")?;
    if item.get_ids().len() > 512 {
        return Err(
            "Le passage est trop long pour ce modèle (limite : 512 unités de texte).".into(),
        );
    }
    let label_id = tokenizer
        .token_to_id("<<LABEL>>")
        .ok_or("Le tokenizer ne reconnaît pas les sujets du modèle.")?;
    let separator_id = tokenizer
        .token_to_id("<<SEP>>")
        .ok_or("Le tokenizer ne reconnaît pas le séparateur du modèle.")?;
    if item.get_ids().is_empty()
        || item.get_ids().iter().filter(|&&id| id == label_id).count() != input.labels.len()
        || item
            .get_ids()
            .iter()
            .filter(|&&id| id == separator_id)
            .count()
            != 1
        || item.get_attention_mask().len() != item.get_ids().len()
        || item.get_attention_mask().iter().any(|&n| n != 1)
    {
        return Err("Les sujets ne sont pas alignés avec l'entrée du modèle.".into());
    }
    Ok(Encoded {
        input_ids: item.get_ids().iter().map(|&x| i64::from(x)).collect(),
        attention_mask: item
            .get_attention_mask()
            .iter()
            .map(|&x| i64::from(x))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use parole_core::{
        Segment,
        topic_questions::{CandidateChoice, PreparedTopicQuestions},
    };
    fn tokenizer() -> Tokenizer {
        let tokens = [("[UNK]", 0), ("[PAD]", 1), ("<<LABEL>>", 3), ("<<SEP>>", 4)];
        let added = tokens.iter().map(|(s,id)| serde_json::json!({"id":id,"content":s,"single_word":false,"lstrip":false,"rstrip":false,"normalized":false,"special":true})).collect::<Vec<_>>();
        let value = serde_json::json!({"version":"1.0","truncation":null,"padding":null,"added_tokens":added,
            "normalizer":null,"pre_tokenizer":{"type":"WhitespaceSplit"},"post_processor":null,"decoder":null,
            "model":{"type":"WordLevel","vocab":{"[UNK]":0,"[PAD]":1,"budget":2,"<<LABEL>>":3,"<<SEP>>":4},"unk_token":"[UNK]"}});
        Tokenizer::from_bytes(serde_json::to_vec(&value).unwrap()).unwrap()
    }
    fn question(text: &str) -> TopicQuestion {
        let source = vec![Segment::new(0, 1000, text.into())];
        PreparedTopicQuestions::prepare("99999999-9999-4999-8999-999999999999", &source)
            .unwrap()
            .question(
                0,
                &[CandidateChoice::Word {
                    term: "budget".into(),
                }],
            )
            .unwrap()
    }
    #[test]
    fn special_literals_are_refused_independently_in_text_and_each_label() {
        let mut t = tokenizer();
        t.add_special_tokens(&[tokenizers::AddedToken::from("__LOCAL_SPECIAL__", true)]);
        let reserved = "Le texte ou un sujet contient un marqueur réservé au modèle.";
        let source = [Segment::new(
            0,
            1000,
            "Le budget et le calendrier du projet Atlas restent à discuter.".into(),
        )];
        let prepared =
            PreparedTopicQuestions::prepare("99999999-9999-4999-8999-999999999999", &source)
                .unwrap();
        let choices = [
            CandidateChoice::Word {
                term: "budget".into(),
            },
            CandidateChoice::Word {
                term: "calendrier".into(),
            },
        ];
        assert!(encode(&t, &prepared.question(0, &choices).unwrap()).is_ok());
        for token in ["[PAD]", "__LOCAL_SPECIAL__"] {
            assert_eq!(
                encode(&t, &question(&format!("budget {token}")))
                    .err()
                    .as_deref(),
                Some(reserved),
            );
            for index in 0..choices.len() {
                let mut q = prepared.question(0, &choices).unwrap();
                let label = format!("{} {token}", q.candidates[index].label);
                // Aligner les deux champs pour ne pas échouer avant le garde testé.
                q.candidates[index].label = label.clone();
                let input = q.model_input.as_mut().unwrap();
                input.labels[index] = label;
                assert!(!input.text.contains(token));
                assert_eq!(input.labels[index], q.candidates[index].label);
                assert_eq!(encode(&t, &q).err().as_deref(), Some(reserved));
            }
        }
    }
    #[test]
    fn inherited_truncation_cannot_hide_a_long_input_or_cut_a_valid_question() {
        let mut t = tokenizer();
        t.with_truncation(Some(tokenizers::TruncationParams {
            max_length: 8,
            ..Default::default()
        }))
        .unwrap();
        let before = t.to_string(false).unwrap();
        let long_source = (0..3)
            .map(|i| {
                Segment::new(
                    i * 1000,
                    i * 1000 + 900,
                    format!("budget {}", "oui ".repeat(200)),
                )
            })
            .collect::<Vec<_>>();
        let long =
            PreparedTopicQuestions::prepare("99999999-9999-4999-8999-999999999999", &long_source)
                .unwrap()
                .question(
                    1,
                    &[CandidateChoice::Word {
                        term: "budget".into(),
                    }],
                )
                .unwrap();
        assert!(encode(&t, &long).is_err());
        let q = question("budget puis parlons de plusieurs choses avant de revenir au budget");
        let full = encode(&tokenizer(), &q).unwrap();
        assert!(full.input_ids.len() > 8);
        assert_eq!(encode(&t, &q).unwrap().input_ids, full.input_ids);
        assert_eq!(t.to_string(false).unwrap(), before);
    }
    #[test]
    fn incomplete_reordered_or_excessive_model_labels_are_refused() {
        for i in 0..4 {
            let mut q = question("Le budget reste à discuter.");
            let input = q.model_input.as_mut().unwrap();
            match i {
                0 => input.labels.clear(),
                1 => input.labels[0] = "   ".into(),
                2 => input.labels = vec!["budget".into(); 26],
                _ => input.labels[0] = "autre sujet".into(),
            }
            assert!(encode(&tokenizer(), &q).is_err(), "label fault {i}");
        }
    }
    #[test]
    fn inherited_padding_does_not_change_a_valid_question() {
        let mut t = tokenizer();
        t.with_padding(Some(tokenizers::PaddingParams {
            strategy: tokenizers::PaddingStrategy::Fixed(80),
            pad_id: 1,
            pad_token: "[PAD]".into(),
            ..Default::default()
        }));
        let before = t.to_string(false).unwrap();
        let q = question("Le budget reste à discuter.");
        assert_eq!(
            encode(&t, &q).unwrap().input_ids,
            encode(&tokenizer(), &q).unwrap().input_ids
        );
        assert_eq!(t.to_string(false).unwrap(), before);
    }
    #[test]
    fn exact_512_token_boundary_is_accepted_and_513_is_refused() {
        let t = tokenizer();
        let build = |count: usize| {
            let source = (0..3)
                .map(|i| {
                    Segment::new(
                        i * 1000,
                        i * 1000 + 900,
                        format!(
                            "budget {}",
                            "oui ".repeat(count / 3 + usize::from((i as usize) < count % 3))
                        ),
                    )
                })
                .collect::<Vec<_>>();
            PreparedTopicQuestions::prepare("99999999-9999-4999-8999-999999999999", &source)
                .unwrap()
                .question(
                    1,
                    &[CandidateChoice::Word {
                        term: "budget".into(),
                    }],
                )
                .unwrap()
        };
        let base = build(0);
        let base_count = encode(&t, &base).unwrap().input_ids.len();
        let boundary = build(512 - base_count);
        assert_eq!(encode(&t, &boundary).unwrap().input_ids.len(), 512);
        assert!(encode(&t, &build(513 - base_count)).is_err());
    }
    #[test]
    fn the_real_question_is_encoded_without_reconstructing_its_text() {
        let q = question("Le budget reste à discuter.");
        let t = tokenizer();
        let expected = t
            .encode(
                format!(
                    "<<LABEL>>{}<<SEP>>{}",
                    q.model_input.as_ref().unwrap().labels[0],
                    q.model_input.as_ref().unwrap().text
                ),
                true,
            )
            .unwrap();
        let actual = encode(&t, &q).unwrap();
        assert_eq!(
            actual.input_ids,
            expected
                .get_ids()
                .iter()
                .map(|&x| i64::from(x))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            actual.attention_mask,
            expected
                .get_attention_mask()
                .iter()
                .map(|&x| i64::from(x))
                .collect::<Vec<_>>()
        );
    }
}
