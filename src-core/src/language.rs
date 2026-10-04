//! Traduction intégrale incrémentale et compte rendu hiérarchique, 100 % locaux.
//!
//! Le moteur réel est `llama-completion` (llama.cpp) avec un modèle GGUF explicite
//! dont l'empreinte SHA-256 est vérifiée. Il fonctionne sur CPU, et demande Metal
//! sur Mac lorsque le binaire natif le prend en charge (à vérifier sur M3).
//! Aucun faux résultat de secours n'est fabriqué en cas d'échec du moteur ;
//! ses propres erreurs factuelles restent possibles et imposent une relecture.
//!
//! Reprise : la traduction est enregistrée dans `Segment::translated_text` et le fichier
//! d'état du travail est réécrit atomiquement après chaque lot ; le compte rendu garde
//! ses sections déjà produites dans un fichier d'état distinct.
use crate::child_process::suppress_child_console;
use crate::{format_timestamp, save_job, Job, Segment};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

// ---------------------------------------------------------------------------
// Langues
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "fr")]
    French,
    #[serde(rename = "en")]
    English,
}

impl Language {
    pub fn parse(code: &str) -> Result<Self, String> {
        match code.trim().to_ascii_lowercase().as_str() {
            "fr" | "fra" | "fre" | "french" | "français" | "francais" => Ok(Self::French),
            "en" | "eng" | "english" | "anglais" => Ok(Self::English),
            _ => Err(format!(
                "Langue non prise en charge : « {} » (français ou anglais uniquement)",
                code.trim()
            )),
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::French => "fr",
            Self::English => "en",
        }
    }
    fn english_name(self) -> &'static str {
        match self {
            Self::French => "French",
            Self::English => "English",
        }
    }
    fn french_name(self) -> &'static str {
        match self {
            Self::French => "français",
            Self::English => "anglais",
        }
    }
}

/// Un choix explicite doit correspondre à la langue des paroles ou à celle
/// de la traduction ; sans texte de référence dans cette langue, le filtrage
/// lexical du compte rendu pourrait supprimer une reformulation fidèle.
pub fn validate_report_choice(
    source_code: &str,
    target_code: Option<&str>,
    report_code: Option<&str>,
) -> Result<(), String> {
    let Some(report_code) = report_code else {
        return Ok(());
    };
    let report = Language::parse(report_code)?;
    if target_code.map(Language::parse).transpose()? == Some(report) {
        return Ok(());
    }
    if source_code == "auto" {
        return Err("Pour choisir la langue des paroles pour le compte rendu, précisez la langue parlée au lieu de la détection automatique".into());
    }
    if Language::parse(source_code)? == report {
        Ok(())
    } else {
        Err("La langue du compte rendu doit être celle des paroles ou de la traduction".into())
    }
}

/// Langue demandée pour le compte rendu ; les anciens travaux conservent
/// le comportement antérieur (traduction, puis langue de la source).
pub fn report_language(job: &Job) -> Result<Language, String> {
    if let Some(code) = job.report_language.as_deref() {
        return Language::parse(code);
    }
    if let Some(code) = job.target_language.as_deref() {
        return Language::parse(code);
    }
    Language::parse(&job.source_language).or_else(|_| {
        let sample = job
            .segments
            .iter()
            .take(80)
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        guess_language(&sample).ok_or_else(|| {
            "Langue source inconnue : choisissez le français ou l'anglais avant de reprendre".into()
        })
    })
}

const FR_WORDS: &[&str] = &[
    "le", "la", "les", "des", "est", "et", "une", "un", "du", "pour", "que", "qui", "pas", "nous",
    "vous", "je", "il", "elle", "ce", "cette", "avec", "dans", "sur", "au", "aux", "mais", "donc",
    "sont", "avons", "c'est", "d'accord", "très", "aussi", "leur", "notre", "votre", "être",
];
const EN_WORDS: &[&str] = &[
    "the", "and", "is", "are", "of", "to", "in", "that", "it", "for", "we", "you", "this", "with",
    "be", "not", "have", "was", "will", "can", "our", "your", "they", "what", "should", "would",
    "let's", "it's", "i'm", "there", "about", "from",
];

/// Estimation grossière (mots-outils) ; `None` si le texte est trop court ou ambigu.
pub fn guess_language(text: &str) -> Option<Language> {
    let (mut fr, mut en) = (0usize, 0usize);
    for raw in text.split(|c: char| !(c.is_alphabetic() || c == '\'' || c == '’')) {
        let word = raw.replace('’', "'").to_lowercase();
        if word.is_empty() {
            continue;
        }
        if FR_WORDS.contains(&word.as_str()) {
            fr += 1;
        }
        if EN_WORDS.contains(&word.as_str()) {
            en += 1;
        }
    }
    if fr + en < 3 {
        return None;
    }
    if fr >= 2 * en.max(1) {
        Some(Language::French)
    } else if en >= 2 * fr.max(1) {
        Some(Language::English)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Modèle épinglé et vérification d'empreinte
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct ModelSpec<'a> {
    pub name: &'a str,
    pub file_name: &'a str,
    pub url: &'a str,
    pub sha256: &'a str,
    pub size_bytes: u64,
    pub license: &'a str,
}

/// Qwen2.5-1.5B-Instruct Q4_K_M (Apache-2.0), révision Hugging Face épinglée.
/// ≈ 1,9 Go de mémoire résidente avec un contexte de 4096 jetons.
pub const MODELE_TEXTE_RECOMMANDE: ModelSpec = ModelSpec {
    name: "Qwen2.5-1.5B-Instruct Q4_K_M",
    file_name: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
    url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/91cad51170dc346986eccefdc2dd33a9da36ead9/qwen2.5-1.5b-instruct-q4_k_m.gguf",
    sha256: "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e",
    size_bytes: 1_117_320_736,
    license: "Apache-2.0",
};

/// Vérifie taille puis SHA-256 ; lecture en flux (jamais le fichier entier en mémoire).
pub fn verify_model_file(path: &Path, spec: &ModelSpec) -> Result<(), String> {
    let meta = fs::metadata(path).map_err(|_| "Modèle de langue introuvable".to_string())?;
    if meta.len() != spec.size_bytes {
        return Err("Modèle de langue incomplet ou différent de la version attendue".into());
    }
    let digest =
        sha256_file(path).map_err(|_| "Lecture du modèle de langue impossible".to_string())?;
    if digest != spec.sha256 {
        return Err("Empreinte du modèle de langue invalide : fichier refusé".into());
    }
    Ok(())
}

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finish_hex())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finish_hex()
}

struct Sha256 {
    state: [u32; 8],
    pending: Vec<u8>,
    length: u64,
}
const K256: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];
impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            pending: Vec::with_capacity(64),
            length: 0,
        }
    }
    fn update(&mut self, mut data: &[u8]) {
        self.length = self.length.wrapping_add(data.len() as u64);
        if !self.pending.is_empty() {
            let take = (64 - self.pending.len()).min(data.len());
            self.pending.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.pending.len() == 64 {
                let block: [u8; 64] = self.pending[..].try_into().unwrap();
                self.compress(&block);
                self.pending.clear();
            }
        }
        let (blocks, remainder) = data.as_chunks::<64>();
        for block in blocks {
            self.compress(block);
        }
        self.pending.extend_from_slice(remainder);
    }
    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (i, word) in block.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K256[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
    fn finish_hex(mut self) -> String {
        let bit_len = self.length.wrapping_mul(8);
        let mut tail = std::mem::take(&mut self.pending);
        tail.push(0x80);
        while tail.len() % 64 != 56 {
            tail.push(0);
        }
        tail.extend_from_slice(&bit_len.to_be_bytes());
        for block in tail.as_chunks::<64>().0 {
            self.compress(block);
        }
        self.state.iter().map(|v| format!("{v:08x}")).collect()
    }
}

// ---------------------------------------------------------------------------
// Moteur de génération
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct GenerationRequest {
    pub system: String,
    pub user: String,
    /// Schéma JSON imposé par grammaire (la sortie est alors syntaxiquement du JSON).
    pub json_schema: Option<String>,
    pub max_tokens: u32,
}

/// Abstraction du moteur ; l'implémentation de production est [`LlamaCppEngine`].
pub trait TextGenerator {
    fn generate(&mut self, request: &GenerationRequest) -> Result<String, String>;
}

#[derive(Clone, Debug)]
pub struct LlamaCppEngine {
    /// Chemin explicite vers `llama-completion` (llama.cpp).
    pub binary: PathBuf,
    pub model: PathBuf,
    /// Dossier des fichiers d'invite temporaires (supprimés après chaque appel).
    pub workspace: PathBuf,
    pub threads: usize,
    pub max_context: u32,
}

static CALL_COUNTER: AtomicU64 = AtomicU64::new(0);

impl LlamaCppEngine {
    pub fn new(binary: PathBuf, model: PathBuf, workspace: PathBuf) -> Self {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, 8);
        Self {
            binary,
            model,
            workspace,
            threads,
            max_context: 8192,
        }
    }
    /// Vérifie la présence du moteur et l'empreinte du modèle avant tout usage.
    pub fn check(&self, spec: &ModelSpec) -> Result<(), String> {
        if !self.binary.is_file() {
            return Err("Moteur de langue local introuvable (llama-completion)".into());
        }
        verify_model_file(&self.model, spec)
    }
}

/// Neutralise les balises spéciales ChatML éventuellement présentes dans la transcription.
fn sanitize(text: &str) -> String {
    text.replace("<|", "‹|").replace("|>", "|›")
}

fn chatml(system: &str, user: &str) -> String {
    format!(
        "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
        sanitize(system),
        sanitize(user)
    )
}

fn estimate_tokens(text: &str) -> u32 {
    // Borne supérieure prudente pour un tokenizer à repli par octets : ne jamais
    // tronquer silencieusement un long passage multilingue faute de contexte.
    u32::try_from(text.len()).unwrap_or(u32::MAX)
}

impl TextGenerator for LlamaCppEngine {
    fn generate(&mut self, request: &GenerationRequest) -> Result<String, String> {
        if !self.binary.is_file() {
            return Err("Moteur de langue local introuvable (llama-completion)".into());
        }
        if !self.model.is_file() {
            return Err("Modèle de langue manquant".into());
        }
        fs::create_dir_all(&self.workspace)
            .map_err(|_| "Dossier de travail inaccessible".to_string())?;
        let id = format!(
            "llm-{}-{}",
            std::process::id(),
            CALL_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let prompt_path = self.workspace.join(format!("{id}.invite.txt"));
        let schema_path = self.workspace.join(format!("{id}.schema.json"));
        let prompt = chatml(&request.system, &request.user);
        let needed = estimate_tokens(&prompt) + request.max_tokens + 256;
        if needed > self.max_context {
            return Err("Passage trop long pour le moteur de langue local".into());
        }
        let ctx = needed.max(2048).div_ceil(256) * 256;
        let write = |path: &Path, content: &str| {
            fs::File::create(path)
                .and_then(|mut f| f.write_all(content.as_bytes()))
                .map_err(|_| "Écriture du fichier d'invite impossible".to_string())
        };
        write(&prompt_path, &prompt)?;
        let mut command = Command::new(&self.binary);
        suppress_child_console(&mut command);
        command
            .arg("-m")
            .arg(&self.model)
            .arg("-f")
            .arg(&prompt_path)
            .args([
                "-no-cnv",
                "--no-display-prompt",
                "--simple-io",
                "--no-warmup",
            ])
            .args(["--temp", "0", "--seed", "42"])
            .args(["-n", &request.max_tokens.to_string()])
            .args(["-c", &ctx.to_string()])
            .args(["-t", &self.threads.max(1).to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Metal est demandé uniquement par le binaire Mac. Son utilisation
        // effective et sa vitesse restent à mesurer sur une installation M3.
        #[cfg(target_os = "macos")]
        command.args(["-ngl", "99"]);
        if let Some(schema) = &request.json_schema {
            if let Err(e) = write(&schema_path, schema) {
                let _ = fs::remove_file(&prompt_path);
                return Err(e);
            }
            command.arg("-jf").arg(&schema_path);
        }
        let output = command.output();
        let _ = fs::remove_file(&prompt_path);
        let _ = fs::remove_file(&schema_path);
        let output =
            output.map_err(|_| "Moteur de langue local impossible à lancer".to_string())?;
        if !output.status.success() {
            return Err("Échec du moteur de langue local".into());
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let text = text.trim_end();
        let text = text.strip_suffix("[end of text]").unwrap_or(text);
        Ok(text.trim().to_string())
    }
}

fn parse_json<T: for<'de> Deserialize<'de>>(raw: &str) -> Result<T, String> {
    let start = raw.find('{');
    let end = raw.rfind('}');
    let slice = match (start, end) {
        (Some(s), Some(e)) if e > s => &raw[s..=e],
        _ => return Err("Réponse du moteur de langue incomplète".into()),
    };
    serde_json::from_str(slice).map_err(|_| "Réponse du moteur de langue illisible".to_string())
}

// ---------------------------------------------------------------------------
// Traduction intégrale incrémentale
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct TranslationOptions {
    /// Langue source ; `None` = celle du travail, sinon estimation sur le texte.
    pub source: Option<Language>,
    pub target: Language,
    /// Budget de caractères source par lot (≈ contexte du modèle).
    pub batch_chars: usize,
    pub max_segments_per_batch: usize,
}

impl TranslationOptions {
    pub fn new(target: Language) -> Self {
        Self {
            source: None,
            target,
            batch_chars: 1200,
            max_segments_per_batch: 12,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TranslationSummary {
    pub total_segments: usize,
    /// Segments traduits pendant cet appel (hors reprise).
    pub translated_now: usize,
    pub already_done: usize,
    /// Lots retraduits segment par segment après une réponse suspecte.
    pub batches_retried: usize,
    /// Segments dont la sortie semble encore dans la langue source : à relire.
    pub suspect_segments: Vec<usize>,
}

fn transcript_language(job: &Job) -> Option<Language> {
    Language::parse(&job.source_language).ok().or_else(|| {
        let sample: String = job
            .segments
            .iter()
            .take(200)
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        guess_language(&sample)
    })
}

/// Oublie les traductions existantes (ex. changement de langue cible) et persiste.
pub fn clear_translations(job: &mut Job, state_path: &Path) -> Result<(), String> {
    for segment in &mut job.segments {
        segment.translated_text = None;
    }
    job.translation_issues.clear();
    save_job(job, state_path).map_err(|_| "Enregistrement de l'état impossible".to_string())
}

fn translation_request(source: Language, target: Language, texts: &[&str]) -> GenerationRequest {
    let n = texts.len();
    let mut user = String::new();
    for (i, text) in texts.iter().enumerate() {
        user.push_str(&format!("[{}] {}\n", i + 1, text.replace('\n', " ")));
    }
    let chars: usize = texts.iter().map(|t| t.chars().count()).sum();
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "translations": {
                "type": "array",
                "items": {"type": "string"},
                "minItems": n,
                "maxItems": n
            }
        },
        "required": ["translations"]
    });
    let terminology = if source == Language::English && target == Language::French {
        "Distinguish launch (lancement) from presentation (présentation), and preserve an event's identity across lines. 'Product launch' means 'lancement du produit', never 'présentation du produit'. 'By Friday morning' means 'd'ici vendredi matin'."
    } else {
        ""
    };
    GenerationRequest {
        system: format!(
            "You are a professional {src}-to-{tgt} translator for meeting transcripts. \
             Translate every numbered line from {src} into {tgt}. Keep the meaning, names, \
             numbers and dates exactly; do not summarize, do not add anything. {terminology} \
             Answer with JSON: {{\"translations\": [...]}} containing exactly {n} strings, \
             one per numbered line, in the same order, all written in {tgt}.",
            src = source.english_name(),
            tgt = target.english_name(),
        ),
        user,
        json_schema: Some(schema.to_string()),
        max_tokens: (chars as u32) / 2 + 24 * n as u32 + 96,
    }
}

#[derive(Deserialize)]
struct TranslationOutput {
    translations: Vec<String>,
}

/// Erreur externe = panne du moteur (propagée) ; erreur interne = réponse mal formée.
fn translate_texts(
    engine: &mut dyn TextGenerator,
    source: Language,
    target: Language,
    texts: &[&str],
) -> Result<Result<Vec<String>, String>, String> {
    let request = translation_request(source, target, texts);
    let raw = engine.generate(&request)?;
    let out: TranslationOutput = match parse_json(&raw) {
        Ok(out) => out,
        Err(e) => return Ok(Err(e)),
    };
    if out.translations.len() != texts.len() {
        return Ok(Err("Nombre de lignes traduites incohérent".into()));
    }
    Ok(Ok(out
        .translations
        .into_iter()
        .map(|t| t.trim().to_string())
        .collect()))
}

fn looks_untranslated(output: &str, source: Language, target: Language) -> bool {
    guess_language(output) == Some(source) && source != target
}

/// Traduit toute la transcription par lots, en persistant après chaque lot.
/// `on_progress(fait, total)` peut renvoyer une erreur pour interrompre proprement.
pub fn translate_job(
    job: &mut Job,
    state_path: &Path,
    engine: &mut dyn TextGenerator,
    options: &TranslationOptions,
    mut on_progress: impl FnMut(usize, usize) -> Result<(), String>,
) -> Result<TranslationSummary, String> {
    if job.segments.is_empty() {
        return Err("Aucune transcription à traduire".into());
    }
    let source = match options.source {
        Some(lang) => lang,
        None => transcript_language(job).ok_or_else(|| {
            "Langue de la transcription inconnue : choisissez français ou anglais".to_string()
        })?,
    };
    if source == options.target {
        return Err(format!(
            "La transcription est déjà en {}",
            options.target.french_name()
        ));
    }
    let batch_chars = options.batch_chars.max(200);
    let max_segments = options.max_segments_per_batch.max(1);
    let total = job.segments.len();
    // Une sortie suspecte d'une exécution précédente n'est jamais réutilisée en silence.
    for &index in &job.translation_issues {
        if let Some(segment) = job.segments.get_mut(index) {
            segment.translated_text = None;
        }
    }
    job.translation_issues.clear();
    save_job(job, state_path).map_err(|_| "Enregistrement de l'état impossible".to_string())?;
    let mut summary = TranslationSummary {
        total_segments: total,
        already_done: job
            .segments
            .iter()
            .filter(|s| s.translated_text.is_some())
            .count(),
        ..Default::default()
    };
    let persist = |job: &Job| {
        save_job(job, state_path).map_err(|_| "Enregistrement de l'état impossible".to_string())
    };

    // Segments vides : rien à traduire, rien à inventer.
    for segment in &mut job.segments {
        if segment.translated_text.is_none() && segment.text.trim().is_empty() {
            segment.translated_text = Some(String::new());
        }
    }

    let mut cursor = 0;
    while cursor < total {
        if job.segments[cursor].translated_text.is_some() {
            cursor += 1;
            continue;
        }
        // Constitution d'un lot contigu de segments non traduits.
        let mut batch = vec![cursor];
        let mut chars = job.segments[cursor].text.chars().count();
        let mut next = cursor + 1;
        while next < total
            && batch.len() < max_segments
            && job.segments[next].translated_text.is_none()
        {
            let len = job.segments[next].text.chars().count();
            if chars + len > batch_chars {
                break;
            }
            chars += len;
            batch.push(next);
            next += 1;
        }
        let texts: Vec<&str> = batch
            .iter()
            .map(|&i| job.segments[i].text.as_str())
            .collect();
        let first = translate_texts(engine, source, options.target, &texts)?;
        let results: Vec<String> = match first {
            Ok(out)
                if batch.len() == 1
                    || !looks_untranslated(&out.join(" "), source, options.target) =>
            {
                out
            }
            Err(error) if batch.len() == 1 => return Err(error),
            _ => {
                // Réponse mal formée ou restée dans la langue source : repli segment par segment.
                summary.batches_retried += 1;
                let mut single = Vec::with_capacity(batch.len());
                for text in &texts {
                    single
                        .push(translate_texts(engine, source, options.target, &[text])??.remove(0));
                }
                single
            }
        };
        for (&index, translated) in batch.iter().zip(results) {
            if translated.trim().is_empty() && !job.segments[index].text.trim().is_empty() {
                return Err(format!(
                    "Traduction vide pour le passage {} : reprise possible",
                    index + 1
                ));
            }
            if looks_untranslated(&translated, source, options.target) {
                summary.suspect_segments.push(index);
                job.translation_issues.push(index);
            }
            job.segments[index].translated_text = Some(translated);
            summary.translated_now += 1;
        }
        persist(job)?;
        let done = job
            .segments
            .iter()
            .filter(|s| s.translated_text.is_some())
            .count();
        on_progress(done, total)?;
        cursor = next;
    }
    persist(job)?;
    Ok(summary)
}

/// Texte traduit horodaté ; un segment non traduit est signalé, jamais comblé.
pub fn render_translated_txt(job: &Job) -> String {
    job.segments
        .iter()
        .map(|segment| {
            let speaker = speaker_label(job, segment).unwrap_or("Locuteur non attribué");
            let text = segment
                .translated_text
                .as_deref()
                .unwrap_or("[traduction manquante]");
            format!(
                "[{}] {} : {}\n",
                format_timestamp(segment.start_ms),
                speaker,
                text
            )
        })
        .collect()
}

fn speaker_label<'a>(job: &'a Job, segment: &'a Segment) -> Option<&'a str> {
    segment
        .speaker_id
        .as_ref()
        .map(|id| job.speaker_names.get(id).map(String::as_str).unwrap_or(id))
}

// ---------------------------------------------------------------------------
// Compte rendu hiérarchique
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionItem {
    pub responsable: String,
    pub tache: String,
    pub echeance: String,
}

/// Niveau 1 : notes d'un passage contigu de la réunion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionNotes {
    pub first_segment: usize,
    pub last_segment: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub titre: String,
    pub resume: String,
    pub points: Vec<String>,
    pub decisions: Vec<String>,
    pub actions: Vec<ActionItem>,
    pub questions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimedText {
    pub texte: String,
    pub start_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimedAction {
    pub responsable: String,
    pub tache: String,
    pub echeance: String,
    pub start_ms: u64,
}

/// Niveau 2 : synthèse globale + listes consolidées (sans réinterprétation par le modèle).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingReport {
    pub language: Language,
    pub titre: String,
    pub synthese: String,
    pub decisions: Vec<TimedText>,
    pub actions: Vec<TimedAction>,
    pub questions: Vec<TimedText>,
    pub sections: Vec<SectionNotes>,
    /// Avertissements en français (ex. langue de sortie douteuse).
    pub avertissements: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReportState {
    pub version: u32,
    pub language: Language,
    pub use_translation: bool,
    pub fingerprint: String,
    pub sections: Vec<SectionNotes>,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub report: Option<MeetingReport>,
}

#[derive(Clone, Debug)]
pub struct ReportOptions {
    pub language: Language,
    /// Utilise `translated_text` (obligatoirement complet) au lieu du texte original.
    pub use_translation: bool,
    pub section_chars: usize,
    pub synthesis_chars: usize,
}

impl ReportOptions {
    pub fn new(language: Language) -> Self {
        Self {
            language,
            use_translation: false,
            section_chars: 2400,
            synthesis_chars: 3000,
        }
    }
}

/// Pour un choix explicite, utilise le texte disponible dans la langue
/// demandée : traduction si elle correspond, paroles originales sinon.
/// Les anciens travaux sans choix explicite gardent leur comportement.
pub fn report_options(job: &Job) -> Result<ReportOptions, String> {
    validate_report_choice(
        &job.source_language,
        job.target_language.as_deref(),
        job.report_language.as_deref(),
    )?;
    let language = report_language(job)?;
    let mut options = ReportOptions::new(language);
    options.use_translation = job.target_language.as_deref() == Some(language.code());
    Ok(options)
}

/// Après une panne de traduction, un rapport peut encore être rédigé à partir
/// des paroles seulement si leur langue a été demandée explicitement. Jamais
/// de repli silencieux vers une autre langue ni après un arrêt volontaire.
pub fn may_build_source_report_after_translation_error(job: &Job, cancelled: bool) -> bool {
    !cancelled
        && job.generate_report
        && job.report.is_none()
        && job.target_language.is_some()
        && job.report_language.is_some()
        && !job.segments.is_empty()
        && report_options(job).is_ok_and(|options| !options.use_translation)
}

/// Des passages traduits douteux bloquent seulement un rapport fondé sur la
/// traduction ; le rapport fondé sur les paroles originales reste possible.
pub fn validate_report_input(job: &Job, options: &ReportOptions) -> Result<(), String> {
    if options.use_translation && !job.translation_issues.is_empty() {
        return Err("Certains passages traduits restent douteux : vérifiez-les avant de produire le compte rendu".into());
    }
    Ok(())
}

const REPORT_STATE_VERSION: u32 = 1;
const NOT_SPECIFIED_FR: &str = "non précisé";
const NOT_SPECIFIED_EN: &str = "not specified";

fn not_specified(language: Language) -> &'static str {
    match language {
        Language::French => NOT_SPECIFIED_FR,
        Language::English => NOT_SPECIFIED_EN,
    }
}

fn fnv1a(hash: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *hash ^= u64::from(*b);
        *hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
}

fn segment_text(segment: &Segment, use_translation: bool) -> Result<&str, String> {
    if use_translation {
        segment
            .translated_text
            .as_deref()
            .ok_or_else(|| "Traduction incomplète : terminez-la avant le compte rendu".to_string())
    } else {
        Ok(segment.text.as_str())
    }
}

fn fingerprint(job: &Job, options: &ReportOptions) -> Result<String, String> {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    fnv1a(&mut hash, options.language.code().as_bytes());
    fnv1a(&mut hash, &[u8::from(options.use_translation)]);
    fnv1a(&mut hash, &(options.section_chars as u64).to_le_bytes());
    // Conserver l'empreinte des anciens travaux sur le modèle de base ; tout
    // modèle optionnel doit invalider les notes et le rapport en cache.
    if job.report_model_id != "baseline" {
        fnv1a(&mut hash, &[0xff]);
        fnv1a(&mut hash, job.report_model_id.as_bytes());
    }
    for segment in &job.segments {
        fnv1a(&mut hash, &segment.start_ms.to_le_bytes());
        fnv1a(&mut hash, &segment.end_ms.to_le_bytes());
        fnv1a(
            &mut hash,
            segment_text(segment, options.use_translation)?.as_bytes(),
        );
        fnv1a(
            &mut hash,
            speaker_label(job, segment).unwrap_or("").as_bytes(),
        );
        fnv1a(&mut hash, &[0]);
    }
    Ok(format!("{hash:016x}"))
}

fn transcript_line(job: &Job, segment: &Segment, text: &str) -> String {
    let stamp = format_timestamp(segment.start_ms);
    let stamp = stamp.split(',').next().unwrap_or("");
    match speaker_label(job, segment) {
        Some(name) => format!("[{stamp}] {name} : {text}\n"),
        None => format!("[{stamp}] {text}\n"),
    }
}

/// Découpe en passages contigus (indices de segments inclusifs).
pub fn plan_sections(job: &Job, max_chars: usize) -> Vec<(usize, usize)> {
    let max_chars = max_chars.max(200);
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = 0;
    for (i, segment) in job.segments.iter().enumerate() {
        let len = segment.text.chars().count() + 14;
        if i > start && chars + len > max_chars {
            out.push((start, i - 1));
            start = i;
            chars = 0;
        }
        chars += len;
    }
    if !job.segments.is_empty() {
        out.push((start, job.segments.len() - 1));
    }
    out
}

fn save_report_state(state: &ReportState, path: &Path) -> Result<(), String> {
    let fail = |_| "Enregistrement du compte rendu impossible".to_string();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(fail)?;
    }
    let tmp = path.with_extension("tmp");
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|_| "Compte rendu non sérialisable".to_string())?;
    let mut file = fs::File::create(&tmp).map_err(fail)?;
    file.write_all(&bytes).map_err(fail)?;
    file.sync_all().map_err(fail)?;
    fs::rename(&tmp, path).map_err(fail)
}

pub fn load_report_state(path: &Path) -> Result<Option<ReportState>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let bytes =
        fs::read(path).map_err(|_| "Lecture de l'état du compte rendu impossible".to_string())?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "État du compte rendu illisible".to_string())
}

#[derive(Deserialize)]
struct RawSection {
    titre: String,
    resume: String,
    #[serde(default)]
    points: Vec<String>,
    #[serde(default)]
    decisions: Vec<String>,
    #[serde(default)]
    actions: Vec<ActionItem>,
    #[serde(default)]
    questions: Vec<String>,
}

fn section_schema() -> String {
    let list = |max: u32| serde_json::json!({"type": "array", "items": {"type": "string", "maxLength": 240}, "maxItems": max});
    serde_json::json!({
        "type": "object",
        "properties": {
            "titre": {"type": "string", "maxLength": 90},
            "resume": {"type": "string", "maxLength": 600},
            "points": list(6),
            "decisions": list(5),
            "actions": {
                "type": "array",
                "maxItems": 5,
                "items": {
                    "type": "object",
                    "properties": {
                        "responsable": {"type": "string", "maxLength": 60},
                        "tache": {"type": "string", "maxLength": 200},
                        "echeance": {"type": "string", "maxLength": 60}
                    },
                    "required": ["responsable", "tache", "echeance"]
                }
            },
            "questions": list(4)
        },
        "required": ["titre", "resume", "points", "decisions", "actions", "questions"]
    })
    .to_string()
}

fn section_system(language: Language, strict: bool) -> String {
    let mut text = match language {
        Language::French => format!(
            "Tu rédiges les notes d'un passage de réunion, en français uniquement, même si la \
             transcription est dans une autre langue. N'invente rien : n'écris que ce qui est dit. \
             « decisions » : décisions explicitement prises. « actions » : tâches confiées ; \
             « responsable » = personne nommée dans le passage, sinon « {ns} » ; « echeance » = \
             date ou délai cité, sinon « {ns} ». « questions » : points restés ouverts. \
             Listes vides si rien ne correspond. « titre » : quelques mots. « resume » : 2 à 4 \
             phrases courtes, sans répétition. Réponds en JSON.",
            ns = NOT_SPECIFIED_FR
        ),
        Language::English => format!(
            "You write notes for one part of a meeting, in English only, even if the transcript \
             is in another language. Invent nothing: only write what is said. \"decisions\": \
             decisions explicitly made. \"actions\": assigned tasks; \"responsable\" = person \
             named in the passage, otherwise \"{ns}\"; \"echeance\" = stated date or deadline, \
             otherwise \"{ns}\". \"questions\": open issues. Empty lists when nothing applies. \
             \"titre\": a few words. \"resume\": 2 to 4 short sentences, no repetition. \
             Answer in JSON.",
            ns = NOT_SPECIFIED_EN
        ),
    };
    if strict {
        text.push_str(match language {
            Language::French => " IMPORTANT : chaque valeur doit être rédigée en français.",
            Language::English => " IMPORTANT: every value must be written in English.",
        });
    }
    text
}

fn section_user(language: Language, passage: &str) -> String {
    match language {
        Language::French => format!("Transcription du passage :\n{passage}"),
        Language::English => format!("Transcript of the passage:\n{passage}"),
    }
}

/// Retire les responsables non cités dans le passage (garde-fou anti-invention).
fn ground_actions(actions: Vec<ActionItem>, passage: &str, language: Language) -> Vec<ActionItem> {
    let haystack = passage.to_lowercase();
    actions
        .into_iter()
        .filter(|a| !a.tache.trim().is_empty())
        .map(|mut a| {
            let who = a.responsable.trim().to_lowercase();
            let cited = !who.is_empty()
                && who
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| w.chars().count() >= 3)
                    .any(|w| haystack.contains(w));
            if !cited {
                a.responsable = not_specified(language).into();
            }
            if a.echeance.trim().is_empty() {
                a.echeance = not_specified(language).into();
            }
            a
        })
        .collect()
}

fn clean_list(items: Vec<String>) -> Vec<String> {
    items
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn section_language_text(raw: &RawSection) -> String {
    let mut text = format!("{} {}", raw.titre, raw.resume);
    for p in &raw.points {
        text.push(' ');
        text.push_str(p);
    }
    text
}

fn summarize_section(
    engine: &mut dyn TextGenerator,
    job: &Job,
    range: (usize, usize),
    options: &ReportOptions,
    warnings: &mut Vec<String>,
) -> Result<SectionNotes, String> {
    let mut passage = String::new();
    for segment in &job.segments[range.0..=range.1] {
        let text = segment_text(segment, options.use_translation)?;
        if !text.trim().is_empty() {
            passage.push_str(&transcript_line(job, segment, text));
        }
    }
    let start_ms = job.segments[range.0].start_ms;
    let end_ms = job.segments[range.1].end_ms;
    if passage.trim().is_empty() {
        return Ok(SectionNotes {
            first_segment: range.0,
            last_segment: range.1,
            start_ms,
            end_ms,
            titre: String::new(),
            resume: String::new(),
            points: vec![],
            decisions: vec![],
            actions: vec![],
            questions: vec![],
        });
    }
    let mut raw: Option<RawSection> = None;
    for strict in [false, true] {
        let request = GenerationRequest {
            system: section_system(options.language, strict),
            user: section_user(options.language, &passage),
            json_schema: Some(section_schema()),
            max_tokens: 900,
        };
        let parsed: RawSection = parse_json(&engine.generate(&request)?)?;
        let drift =
            guess_language(&section_language_text(&parsed)).is_some_and(|l| l != options.language);
        raw = Some(parsed);
        if !drift {
            break;
        }
        if strict {
            warnings.push(format!(
                "Passage {} – {} : langue des notes douteuse, à relire",
                &format_timestamp(start_ms)[..8],
                &format_timestamp(end_ms)[..8]
            ));
        }
    }
    let raw = raw.ok_or_else(|| "Réponse du moteur de langue absente".to_string())?;
    Ok(SectionNotes {
        first_segment: range.0,
        last_segment: range.1,
        start_ms,
        end_ms,
        titre: raw.titre.trim().to_string(),
        resume: dedupe_sentences(&raw.resume),
        points: clean_list(raw.points),
        decisions: clean_list(raw.decisions),
        actions: ground_actions(raw.actions, &passage, options.language),
        questions: clean_list(raw.questions),
    })
}

#[derive(Deserialize)]
struct RawSummary {
    #[serde(default)]
    titre: String,
    synthese: String,
}

fn synthesis_request(
    language: Language,
    material: &str,
    with_title: bool,
    strict: bool,
) -> GenerationRequest {
    let mut system = match language {
        Language::French => "Tu rédiges en français la synthèse fidèle d'une réunion à partir de \
             résumés de passages horodatés. N'ajoute aucune information absente des résumés. \
             « synthese » : 4 à 8 phrases, chaque fait une seule fois. Réponds en JSON."
            .to_string(),
        Language::English => "You write, in English, a faithful summary of a meeting from \
             timestamped passage summaries. Add no information absent from the summaries. \
             \"synthese\": 4 to 8 sentences, each fact stated once. Answer in JSON."
            .to_string(),
    };
    if with_title {
        system.push_str(match language {
            Language::French => " « titre » : titre court et factuel de la réunion.",
            Language::English => " \"titre\": short factual title of the meeting.",
        });
    }
    if strict {
        system.push_str(match language {
            Language::French => " IMPORTANT : rédige uniquement en français.",
            Language::English => " IMPORTANT: write only in English.",
        });
    }
    let mut properties = serde_json::Map::new();
    let mut required = vec![serde_json::json!("synthese")];
    if with_title {
        properties.insert(
            "titre".into(),
            serde_json::json!({"type": "string", "maxLength": 90}),
        );
        required.insert(0, serde_json::json!("titre"));
    }
    properties.insert(
        "synthese".into(),
        serde_json::json!({"type": "string", "maxLength": 1400}),
    );
    let schema =
        serde_json::json!({"type": "object", "properties": properties, "required": required});
    GenerationRequest {
        system,
        user: material.to_string(),
        json_schema: Some(schema.to_string()),
        max_tokens: 700,
    }
}

fn synthesize(
    engine: &mut dyn TextGenerator,
    language: Language,
    material: &str,
    with_title: bool,
    warnings: &mut Vec<String>,
) -> Result<RawSummary, String> {
    let mut last = None;
    for strict in [false, true] {
        let parsed: RawSummary = parse_json(
            &engine.generate(&synthesis_request(language, material, with_title, strict))?,
        )?;
        let drift = guess_language(&parsed.synthese).is_some_and(|l| l != language);
        last = Some(parsed);
        if !drift {
            break;
        }
        if strict {
            warnings.push("Synthèse : langue douteuse, à relire".into());
        }
    }
    last.ok_or_else(|| "Réponse du moteur de langue absente".to_string())
}

/// Réduction hiérarchique : regroupe les résumés tant qu'ils dépassent le budget.
fn reduce_summaries(
    engine: &mut dyn TextGenerator,
    language: Language,
    mut items: Vec<String>,
    budget: usize,
    warnings: &mut Vec<String>,
) -> Result<RawSummary, String> {
    let budget = budget.max(400);
    for _level in 0..8 {
        let total: usize = items.iter().map(|s| s.chars().count() + 1).sum();
        if total <= budget || items.len() <= 1 {
            return synthesize(engine, language, &items.join("\n"), true, warnings);
        }
        let mut next = Vec::new();
        // Chaque groupe contient au moins deux résumés : le nombre d'éléments diminue
        // forcément à chaque niveau, même si un résumé dépasse la moitié du budget.
        let mut group = String::new();
        let mut in_group = 0usize;
        for item in &items {
            if in_group >= 2 && group.chars().count() + item.chars().count() > budget {
                next.push(synthesize(engine, language, &group, false, warnings)?.synthese);
                group.clear();
                in_group = 0;
            }
            group.push_str(item);
            group.push('\n');
            in_group += 1;
        }
        if in_group == 1 && !next.is_empty() {
            // Dernier résumé isolé : repris tel quel au niveau suivant.
            next.push(group.trim_end().to_string());
        } else if in_group > 0 {
            next.push(synthesize(engine, language, &group, false, warnings)?.synthese);
        }
        if next.len() >= items.len() {
            return Err("Réduction du compte rendu impossible (passages trop longs)".into());
        }
        items = next;
    }
    Err("Réduction du compte rendu impossible".into())
}

/// Supprime les phrases répétées mot pour mot (défaut fréquent des petits modèles).
pub fn dedupe_sentences(text: &str) -> String {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = String::new();
    let mut current = String::new();
    let mut flush = |sentence: &mut String, out: &mut String| {
        let trimmed = sentence.trim();
        if !trimmed.is_empty() && seen.insert(normalized(trimmed)) {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(trimmed);
        }
        sentence.clear();
    };
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        if matches!(c, '.' | '!' | '?') && chars.peek().is_none_or(|n| n.is_whitespace()) {
            flush(&mut current, &mut out);
        }
    }
    flush(&mut current, &mut out);
    out
}

fn normalized(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn consolidate(sections: &[SectionNotes]) -> (Vec<TimedText>, Vec<TimedAction>, Vec<TimedText>) {
    let mut seen = std::collections::BTreeSet::new();
    let mut decisions = Vec::new();
    let mut actions = Vec::new();
    let mut questions = Vec::new();
    for section in sections {
        for d in &section.decisions {
            if seen.insert(format!("d:{}", normalized(d))) {
                decisions.push(TimedText {
                    texte: d.clone(),
                    start_ms: section.start_ms,
                });
            }
        }
        for a in &section.actions {
            if seen.insert(format!("a:{}", normalized(&a.tache))) {
                actions.push(TimedAction {
                    responsable: a.responsable.clone(),
                    tache: a.tache.clone(),
                    echeance: a.echeance.clone(),
                    start_ms: section.start_ms,
                });
            }
        }
        for q in &section.questions {
            if seen.insert(format!("q:{}", normalized(q))) {
                questions.push(TimedText {
                    texte: q.clone(),
                    start_ms: section.start_ms,
                });
            }
        }
    }
    (decisions, actions, questions)
}

/// Relit un rapport terminé seulement si le cache correspond encore aux entrées.
/// L'empreinte historique établit la compatibilité, pas l'authenticité d'un fichier hostile.
pub fn load_completed_report(
    job: &Job,
    report_state_path: &Path,
    options: &ReportOptions,
) -> Result<Option<MeetingReport>, String> {
    if job.segments.iter().all(|s| s.text.trim().is_empty())
        || validate_report_input(job, options).is_err()
    {
        return Ok(None);
    }
    // Une traduction à terminer ne doit pas être bloquée par la tentative de récupération.
    let Ok(expected) = fingerprint(job, options) else {
        return Ok(None);
    };
    let Some(state) = load_report_state(report_state_path)? else {
        return Ok(None);
    };
    if state.version != REPORT_STATE_VERSION
        || state.fingerprint != expected
        || state.language != options.language
        || state.use_translation != options.use_translation
    {
        return Ok(None);
    }
    Ok(state
        .report
        .filter(|report| report.language == options.language))
}

/// Produit (ou reprend) le compte rendu hiérarchique. L'état est persisté après chaque
/// passage ; une transcription modifiée invalide automatiquement les passages déjà faits.
pub fn build_report(
    job: &Job,
    report_state_path: &Path,
    engine: &mut dyn TextGenerator,
    options: &ReportOptions,
    mut on_progress: impl FnMut(usize, usize) -> Result<(), String>,
) -> Result<MeetingReport, String> {
    if job.segments.iter().all(|s| s.text.trim().is_empty()) {
        return Err("Aucune transcription pour le compte rendu".into());
    }
    let fingerprint = fingerprint(job, options)?;
    let mut state = match load_report_state(report_state_path)? {
        Some(s)
            if s.version == REPORT_STATE_VERSION
                && s.fingerprint == fingerprint
                && s.language == options.language
                && s.use_translation == options.use_translation =>
        {
            s
        }
        _ => ReportState {
            version: REPORT_STATE_VERSION,
            language: options.language,
            use_translation: options.use_translation,
            fingerprint,
            sections: vec![],
            warnings: vec![],
            report: None,
        },
    };
    if let Some(report) = &state.report {
        return Ok(report.clone());
    }
    let plan = plan_sections(job, options.section_chars);
    let total_steps = plan.len() + 1;
    let mut warnings = std::mem::take(&mut state.warnings);
    if !options.use_translation && state.sections.is_empty() {
        if let Some(source) = transcript_language(job).filter(|l| *l != options.language) {
            warnings.push(format!(
                "Transcription en {} résumée directement en {} : pour un résultat plus fiable, \
                 traduisez d'abord la transcription puis générez le compte rendu depuis la traduction",
                source.french_name(),
                options.language.french_name()
            ));
        }
    }
    for (index, range) in plan.iter().enumerate() {
        if index < state.sections.len() {
            continue;
        }
        let notes = summarize_section(engine, job, *range, options, &mut warnings)?;
        state.sections.push(notes);
        state.warnings = warnings.clone();
        save_report_state(&state, report_state_path)?;
        on_progress(index + 1, total_steps)?;
    }
    let items: Vec<String> = state
        .sections
        .iter()
        .filter(|s| !s.resume.is_empty())
        .map(|s| {
            format!(
                "[{} – {}] {} : {}",
                &format_timestamp(s.start_ms)[..8],
                &format_timestamp(s.end_ms)[..8],
                s.titre,
                s.resume
            )
        })
        .collect();
    let summary = reduce_summaries(
        engine,
        options.language,
        items,
        options.synthesis_chars,
        &mut warnings,
    )?;
    let (decisions, actions, questions) = consolidate(&state.sections);
    let report = MeetingReport {
        language: options.language,
        titre: summary.titre.trim().to_string(),
        synthese: dedupe_sentences(&summary.synthese),
        decisions,
        actions,
        questions,
        sections: state.sections.clone(),
        avertissements: warnings,
    };
    state.report = Some(report.clone());
    save_report_state(&state, report_state_path)?;
    on_progress(total_steps, total_steps)?;
    Ok(report)
}

fn hms(ms: u64) -> String {
    format_timestamp(ms)[..8].to_string()
}

/// Encapsule une valeur non fiable dans une ligne Markdown, sans créer de
/// nouveau titre, liste, tableau, image ou lien. Le texte visible est décodé
/// uniquement sur les rapports marqués comme produits dans ce format.
pub(crate) fn markdown_inline_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        out.push_str(match ch {
            '\n' | '\r' | '\t' => " ",
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '!' => "&#33;",
            ':' => "&#58;",
            '.' => "&#46;",
            '@' => "&#64;",
            '[' => "&#91;",
            ']' => "&#93;",
            '(' => "&#40;",
            ')' => "&#41;",
            '*' => "&#42;",
            '_' => "&#95;",
            '`' => "&#96;",
            '~' => "&#126;",
            '\\' => "&#92;",
            '|' => "&#124;",
            '#' => "&#35;",
            '+' => "&#43;",
            '-' => "&#45;",
            '{' => "&#123;",
            '}' => "&#125;",
            _ => {
                out.push(ch);
                continue;
            }
        });
    }
    out
}

/// Rendu Markdown du compte rendu (intitulés dans la langue du compte rendu).
pub fn render_report_markdown(report: &MeetingReport) -> String {
    render_report_markdown_with_presentation(report, "", None, None, None, false)
}

/// Les avertissements et titres contrôlés sont insérés à leur place dans la
/// structure, jamais par remplacement de sous-chaînes dans du texte de modèle.
pub(crate) fn render_report_markdown_with_presentation(
    report: &MeetingReport,
    before_summary: &str,
    decisions_title: Option<&str>,
    actions_title: Option<&str>,
    questions_title: Option<&str>,
    hide_empty_outline: bool,
) -> String {
    let fr = report.language == Language::French;
    let l = |f: &'static str, e: &'static str| if fr { f } else { e };
    let mut out = String::new();
    let title = if report.titre.is_empty() {
        l("Compte rendu de réunion", "Meeting report")
    } else {
        report.titre.as_str()
    };
    out.push_str(&format!("# {}\n\n", markdown_inline_literal(title)));
    out.push_str(l(
        "_Compte rendu généré localement par un modèle de langue : à relire avant diffusion._\n\n",
        "_Report generated locally by a language model: review before sharing._\n\n",
    ));
    if !report.avertissements.is_empty() {
        out.push_str(&format!("## {}\n\n", l("Avertissements", "Warnings")));
        for w in &report.avertissements {
            out.push_str(&format!("- {}\n", markdown_inline_literal(w)));
        }
        out.push('\n');
    }
    out.push_str(before_summary);
    out.push_str(&format!(
        "## {}\n\n{}\n\n",
        l("Synthèse", "Summary"),
        markdown_inline_literal(&report.synthese)
    ));
    let none = l("_Aucune._", "_None._");
    out.push_str(&format!(
        "## {}\n\n",
        decisions_title.unwrap_or(l("Décisions", "Decisions"))
    ));
    if report.decisions.is_empty() {
        out.push_str(&format!("{none}\n"));
    }
    for d in &report.decisions {
        out.push_str(&format!(
            "- [{}] {}\n",
            hms(d.start_ms),
            markdown_inline_literal(&d.texte)
        ));
    }
    out.push_str(&format!(
        "\n## {}\n\n",
        actions_title.unwrap_or(l("Actions", "Action items"))
    ));
    if report.actions.is_empty() {
        out.push_str(&format!("{none}\n"));
    } else {
        out.push_str(l(
            "| Responsable | Tâche | Échéance | Moment |\n|---|---|---|---|\n",
            "| Owner | Task | Due | Time |\n|---|---|---|---|\n",
        ));
        let cell = markdown_inline_literal;
        for a in &report.actions {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                cell(&a.responsable),
                cell(&a.tache),
                cell(&a.echeance),
                hms(a.start_ms)
            ));
        }
    }
    out.push_str(&format!(
        "\n## {}\n\n",
        questions_title.unwrap_or(l("Questions ouvertes", "Open questions"))
    ));
    if report.questions.is_empty() {
        out.push_str(&format!("{none}\n"));
    }
    for q in &report.questions {
        out.push_str(&format!(
            "- [{}] {}\n",
            hms(q.start_ms),
            markdown_inline_literal(&q.texte)
        ));
    }
    if !hide_empty_outline || !report.sections.is_empty() {
        out.push_str(&format!(
            "\n## {}\n",
            l("Déroulé détaillé", "Detailed outline")
        ));
    }
    for (i, s) in report.sections.iter().enumerate() {
        let titre = if s.titre.is_empty() {
            l("(passage sans contenu)", "(empty passage)")
        } else {
            s.titre.as_str()
        };
        out.push_str(&format!(
            "\n### {}. {} ({} – {})\n\n",
            i + 1,
            markdown_inline_literal(titre),
            hms(s.start_ms),
            hms(s.end_ms)
        ));
        if !s.resume.is_empty() {
            out.push_str(&format!("{}\n", markdown_inline_literal(&s.resume)));
        }
        if !s.points.is_empty() {
            out.push('\n');
            for p in &s.points {
                out.push_str(&format!("- {}\n", markdown_inline_literal(p)));
            }
        }
    }
    out
}
