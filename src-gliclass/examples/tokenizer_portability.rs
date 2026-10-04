//! Compare le découpage des mêmes phrases fictives. Aucun moteur ONNX ni score.
#[path = "../src/encoding.rs"]
mod encoding;
use parole_core::{
    Segment,
    topic_questions::{CandidateChoice, PreparedTopicQuestions, TopicQuestion},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::PathBuf};
use tokenizers::Tokenizer;
const ID: &str = "23232323-2323-4323-8323-232323232323";
fn prepared_question(text: &str) -> Result<TopicQuestion, String> {
    prepared_parts(text, "", "")
}
fn prepared_parts(text: &str, before: &str, after: &str) -> Result<TopicQuestion, String> {
    let segments = vec![
        Segment::new(
            0,
            1000,
            format!("Le projet Atlas prépare le budget. {before}"),
        ),
        Segment::new(
            1000,
            2000,
            format!("Le calendrier du projet Atlas reste à discuter. {text}"),
        ),
        Segment::new(
            2000,
            3000,
            format!("Le dossier Luciole conserve les illustrations. {after}"),
        ),
    ];
    PreparedTopicQuestions::prepare(ID, &segments)?.question(
        1,
        &[
            CandidateChoice::WordInPossibleFolder {
                term: "budget".into(),
                folder: "Atlas".into(),
            },
            CandidateChoice::WordInPossibleFolder {
                term: "calendrier".into(),
                folder: "Atlas".into(),
            },
            CandidateChoice::WordInPossibleFolder {
                term: "illustrations".into(),
                folder: "Luciole".into(),
            },
        ],
    )
}
fn question(text: &str) -> TopicQuestion {
    prepared_question(text).unwrap()
}
fn input_row(name: &str, t: &Tokenizer, text: &str) -> Value {
    match prepared_question(text) {
        Ok(q) => row(name, t, &q),
        Err(error) => {
            json!({"name":name,"status":"source-rejected","source_fixture":text,"error":error})
        }
    }
}
fn row(name: &str, t: &Tokenizer, q: &TopicQuestion) -> Value {
    match encoding::encode(t, q) {
        Ok(value) => {
            json!({"name":name,"question":q,"status":"encoded","count":value.input_ids.len(),"input_ids":value.input_ids,"attention_mask":value.attention_mask})
        }
        Err(error) => json!({"name":name,"question":q,"status":"rejected","error":error}),
    }
}
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    assert_eq!(args.len(), 3);
    assert_eq!(args[2], "fictional-fixture-only");
    let path = PathBuf::from(&args[1]);
    assert!(path.is_absolute());
    let mut bytes = Vec::new();
    File::open(&path)
        .unwrap()
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= 32 * 1024 * 1024);
    let tokenizer = Tokenizer::from_bytes(&bytes).unwrap();
    let cases = [
        ("simple", "Il faut encore en discuter."),
        ("spaces", "  Nous     gardons   les  mots.  "),
        ("whitespace", "Entre\tles mots\nune ligne\r\nune autre."),
        (
            "accent",
            "Échéance, coût, façade, cœur, déjà ; e\u{301}tude.",
        ),
        ("unicode", "中文、日本語、العربية、Ελληνικά、Українська."),
        ("emoji", "👨‍👩‍👧‍👦 🚀 🧑🏽‍🎨 𠮷"),
        ("numbers", "12 345,67 € ; -0,5 % ; AB-123 ; 2026/09/30."),
        ("invisible", "A\u{a0}B\u{202f}C\u{200b}D\u{2060}E"),
        (
            "punctuation",
            "« Oui ? » : {a:[b]} \"c\" ; l’idée / l'idee.",
        ),
        ("replacement", "ＡＢＣ  １２３   ﬁ ﬀ ① K Å"),
        ("controls", "Un zéro\0 et une tabulation\t puis la suite."),
    ];
    let mut rows: Vec<_> = cases
        .iter()
        .map(|(n, s)| input_row(n, &tokenizer, s))
        .collect();
    assert!(rows.iter().all(|r| r["status"] == "encoded"));
    let mut inherited = tokenizer.clone();
    inherited
        .with_truncation(Some(tokenizers::TruncationParams {
            max_length: 8,
            ..Default::default()
        }))
        .unwrap();
    inherited.with_padding(Some(tokenizers::PaddingParams {
        strategy: tokenizers::PaddingStrategy::Fixed(512),
        ..Default::default()
    }));
    let unchanged = row("inherited-options", &inherited, &question(cases[0].1));
    assert_eq!(unchanged["input_ids"], rows[0]["input_ids"]);
    assert_eq!(unchanged["attention_mask"], rows[0]["attention_mask"]);
    rows.push(unchanged);
    let reserved = "Le texte ou un sujet contient un marqueur réservé au modèle.";
    for marker in [
        "[PAD]",
        "[CLS]",
        "[SEP]",
        "[UNK]",
        "[MASK]",
        "<<LABEL>>",
        "<<SEP>>",
        "<<EXAMPLE>>",
    ] {
        let out = input_row(&format!("reserved-text-{marker}"), &tokenizer, marker);
        if matches!(marker, "<<LABEL>>" | "<<SEP>>") {
            assert_eq!(out["status"], "source-rejected");
            assert_eq!(
                out["error"],
                "Le passage ou son voisinage ne peut pas être transmis tel quel au classifieur."
            );
        } else {
            assert_eq!(out["error"], reserved);
        }
        rows.push(out);
        for index in 0..3 {
            let mut q = question("Les sujets restent ouverts.");
            q.candidates[index].label.push_str(marker);
            q.model_input.as_mut().unwrap().labels[index] = q.candidates[index].label.clone();
            let out = row(&format!("reserved-label-{index}-{marker}"), &tokenizer, &q);
            assert_eq!(out["error"], reserved);
            rows.push(out);
        }
    }
    let mut dynamic = tokenizer.clone();
    dynamic.add_special_tokens(&[tokenizers::AddedToken::from("__FICTIONAL_SPECIAL__", true)]);
    let out = row(
        "dynamic-special",
        &dynamic,
        &question("__FICTIONAL_SPECIAL__"),
    );
    assert_eq!(out["error"], reserved);
    rows.push(out);
    // Répartir le texte entre les trois voisins : chacun reste sous la limite de 256 mots.
    let boundary_question = |n: usize| {
        prepared_parts(
            &"mot ".repeat(n / 3),
            &"mot ".repeat(n / 3 + usize::from(!n.is_multiple_of(3))),
            &"mot ".repeat(n / 3 + usize::from(n % 3 > 1)),
        )
        .unwrap()
    };
    let raw_ids = |q: &TopicQuestion| {
        let input = q.model_input.as_ref().unwrap();
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
        complete.with_truncation(None).unwrap();
        complete.with_padding(None);
        complete.encode(prompt, true).unwrap().get_ids().to_vec()
    };
    let (mut low, mut high) = (0, 600);
    assert!(
        raw_ids(&boundary_question(low)).len() < 512
            && raw_ids(&boundary_question(high)).len() > 512
    );
    while low < high {
        let middle = (low + high) / 2;
        if raw_ids(&boundary_question(middle)).len() < 512 {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let n = low;
    let q = boundary_question(n);
    let exact = raw_ids(&q);
    assert_eq!(exact.len(), 512);
    let value = row("exact-512", &tokenizer, &q);
    assert_eq!(value["input_ids"], json!(exact));
    rows.push(value);
    let next = boundary_question(n + 1);
    let exact = raw_ids(&next);
    assert_eq!(exact.len(), 513);
    let mut out = row("exact-513-rejected", &tokenizer, &next);
    assert_eq!(
        out["error"],
        "Le passage est trop long pour ce modèle (limite : 512 unités de texte)."
    );
    out["unbounded_input_ids"] = json!(exact);
    rows.push(out);
    println!(
        "{}",
        json!({"tag":"parole-tokenizer-portability-v1","tokenizer_sha256":format!("{:x}",Sha256::digest(bytes)),"boundary_repetitions":n,"cases":rows,"native_model_loaded":false,"model_inferences":0})
    );
}
