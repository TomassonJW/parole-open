//! Diarisation locale réelle (CPU) par tranches, avec identités de locuteurs
//! cohérentes d'une tranche à l'autre.
//!
//! Chaîne d'inférence (aucune attribution simulée) :
//! 1. segmentation pyannote 3.0 (ONNX, MIT) + regroupement local, exécutés par
//!    la bibliothèque native `sherpa-onnx` (Apache-2.0, API C, chargée à
//!    l'exécution par `dlopen`, aucune dépendance Cargo supplémentaire) ;
//! 2. pour chaque locuteur local d'une tranche, calcul d'une empreinte vocale
//!    (modèle d'embedding ONNX, par ex. 3D-Speaker ERes2Net Apache-2.0 ou
//!    WeSpeaker ResNet34 CC-BY-4.0) sur ses tours de parole sans chevauchement ;
//! 3. appariement un-à-un de ces empreintes avec le registre global
//!    (similarité cosinus ≥ seuil), sinon création d'un nouveau locuteur.
//!
//! Le registre est sérialisable : une reprise après interruption conserve
//! les mêmes identifiants. Si la bibliothèque ou les modèles manquent, les
//! fonctions renvoient une erreur explicite ; aucun locuteur n'est inventé.
//!
//! Le test d'intégration charge ce module avec l'utilitaire de sous-processus
//! pour exercer le même décodage local que la bibliothèque.

use crate::child_process::suppress_child_console;
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::ffi::c_int;
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Les médias fournis restent locaux, y compris lorsque leur conteneur référence d'autres flux.
pub const LOCAL_PROTOCOLS: &str = "file,pipe";
/// Fréquence attendue par les modèles de segmentation et d'embedding.
pub const SAMPLE_RATE: u32 = 16_000;
/// Versions de l'API C dont la disposition mémoire des structures a été
/// vérifiée contre `c-api.h`.
pub const SUPPORTED_SHERPA_PREFIXES: &[&str] = &["1.13.8"];
/// Préfixe lisible des identifiants globaux (affiché tel quel par l'UI tant
/// qu'aucun nom n'a été saisi).
pub const SPEAKER_PREFIX: &str = "Locuteur";

#[derive(Debug, thiserror::Error)]
pub enum DiarizationError {
    #[error("Bibliothèque de diarisation introuvable ou illisible : {0}")]
    Library(String),
    #[error("Version de la bibliothèque de diarisation non prise en charge : {0}")]
    UnsupportedVersion(String),
    #[error("Modèle de diarisation introuvable : {0}")]
    MissingModel(PathBuf),
    #[error("Configuration de diarisation invalide : {0}")]
    InvalidConfig(String),
    #[error("Initialisation du moteur de diarisation impossible")]
    InitFailed,
    #[error("Décodage audio pour la diarisation impossible : {0}")]
    Decode(String),
    #[error("Échec de l'inférence de diarisation : {0}")]
    Inference(String),
}

pub type Result<T> = std::result::Result<T, DiarizationError>;

/// Emplacements et réglages du moteur natif.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiarizationConfig {
    /// Chemin de `libsherpa-onnx-c-api.so` (libonnxruntime doit être à côté).
    pub sherpa_library: PathBuf,
    /// Modèle de segmentation pyannote (`model.onnx` ou `model.int8.onnx`).
    pub segmentation_model: PathBuf,
    /// Modèle d'empreinte vocale 16 kHz.
    pub embedding_model: PathBuf,
    pub num_threads: i32,
    /// Seuil de regroupement intra-tranche (distance, API sherpa-onnx).
    pub clustering_threshold: f32,
    /// Nombre de locuteurs connu à l'avance (≤ 0 : inconnu).
    pub num_speakers: i32,
    pub min_duration_on_s: f32,
    pub min_duration_off_s: f32,
    /// Similarité cosinus minimale pour rattacher un locuteur local à un
    /// locuteur global déjà connu.
    pub match_threshold: f32,
    /// Parole propre minimale pour calculer une empreinte fiable.
    pub min_embedding_ms: u64,
    /// Parole maximale utilisée par empreinte (borne le coût CPU).
    pub max_embedding_ms: u64,
}

impl DiarizationConfig {
    /// Réglages calibrés pour 3D-Speaker ERes2Net (voxceleb, 16 kHz).
    pub fn new(
        sherpa_library: PathBuf,
        segmentation_model: PathBuf,
        embedding_model: PathBuf,
    ) -> Self {
        Self {
            sherpa_library,
            segmentation_model,
            embedding_model,
            num_threads: 2,
            clustering_threshold: 0.8,
            num_speakers: -1,
            min_duration_on_s: 0.3,
            min_duration_off_s: 0.5,
            match_threshold: 0.5,
            min_embedding_ms: 500,
            max_embedding_ms: 20_000,
        }
    }

    pub fn validate(&self) -> Result<()> {
        for model in [&self.segmentation_model, &self.embedding_model] {
            if !model.is_file() {
                return Err(DiarizationError::MissingModel(model.clone()));
            }
        }
        if !self.sherpa_library.is_file() {
            return Err(DiarizationError::Library(
                self.sherpa_library.display().to_string(),
            ));
        }
        if !(self.match_threshold > 0.0 && self.match_threshold < 1.0) {
            return Err(DiarizationError::InvalidConfig(
                "seuil d'appariement hors ]0, 1[".into(),
            ));
        }
        if self.num_threads < 1 || self.clustering_threshold <= 0.0 {
            return Err(DiarizationError::InvalidConfig(
                "fils ou seuil de regroupement".into(),
            ));
        }
        Ok(())
    }
}

/// Tour de parole d'un locuteur, en millisecondes absolues dans le média.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeakerTurn {
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker_id: String,
}

/// Tour brut produit par l'inférence sur une tranche (étiquette locale).
#[derive(Clone, Debug, PartialEq)]
pub struct LocalTurn {
    pub start_s: f32,
    pub end_s: f32,
    pub local_speaker: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GlobalSpeaker {
    pub id: String,
    /// Somme pondérée (en secondes) des empreintes normalisées.
    pub embedding_sum: Vec<f32>,
    pub total_ms: u64,
}

impl GlobalSpeaker {
    pub fn centroid(&self) -> Vec<f32> {
        normalized(&self.embedding_sum)
    }
}

/// Registre des locuteurs connus pour un média ; à conserver avec l'état du
/// travail pour garder les identifiants lors d'une reprise.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SpeakerRegistry {
    pub speakers: Vec<GlobalSpeaker>,
    /// Tranches déjà diarisées (évite de compter deux fois une tranche).
    pub processed_chunks: Vec<usize>,
}

impl SpeakerRegistry {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let bytes =
            std::fs::read(path).map_err(|e| DiarizationError::InvalidConfig(e.to_string()))?;
        serde_json::from_slice(&bytes).map_err(|e| DiarizationError::InvalidConfig(e.to_string()))
    }

    /// Écriture atomique (fichier temporaire puis renommage).
    pub fn save(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| DiarizationError::InvalidConfig(e.to_string()))?;
        let mut file = std::fs::File::create(&tmp)
            .map_err(|e| DiarizationError::InvalidConfig(e.to_string()))?;
        use std::io::Write;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| DiarizationError::InvalidConfig(e.to_string()))?;
        std::fs::rename(&tmp, path).map_err(|e| DiarizationError::InvalidConfig(e.to_string()))
    }

    fn next_id(&self) -> String {
        format!("{SPEAKER_PREFIX} {}", self.speakers.len() + 1)
    }

    /// Apparie un-à-un des empreintes locales (normalisées, avec leur durée)
    /// au registre. Renvoie l'identifiant global de chaque entrée.
    pub fn match_embeddings(&mut self, locals: &[(Vec<f32>, u64)], threshold: f32) -> Vec<String> {
        let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
        for (li, (emb, _)) in locals.iter().enumerate() {
            for (gi, g) in self.speakers.iter().enumerate() {
                let sim = cosine(emb, &g.centroid());
                if sim >= threshold {
                    pairs.push((sim, li, gi));
                }
            }
        }
        // Ordre déterministe : similarité décroissante puis indices.
        pairs.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let mut local_to_global: Vec<Option<usize>> = vec![None; locals.len()];
        let mut used = vec![false; self.speakers.len()];
        for (_, li, gi) in pairs {
            if local_to_global[li].is_none() && !used[gi] {
                local_to_global[li] = Some(gi);
                used[gi] = true;
            }
        }
        let mut ids = Vec::with_capacity(locals.len());
        for (li, (emb, ms)) in locals.iter().enumerate() {
            let weight = (*ms as f32 / 1000.0).max(1e-3);
            let gi = match local_to_global[li] {
                Some(gi) => gi,
                None => {
                    self.speakers.push(GlobalSpeaker {
                        id: self.next_id(),
                        embedding_sum: vec![0.0; emb.len()],
                        total_ms: 0,
                    });
                    self.speakers.len() - 1
                }
            };
            let g = &mut self.speakers[gi];
            if g.embedding_sum.len() == emb.len() {
                for (s, v) in g.embedding_sum.iter_mut().zip(emb) {
                    *s += v * weight;
                }
                g.total_ms += ms;
            }
            ids.push(g.id.clone());
        }
        ids
    }
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return -1.0;
    }
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in a.iter().zip(b) {
        dot += (*x as f64) * (*y as f64);
        na += (*x as f64).powi(2);
        nb += (*y as f64).powi(2);
    }
    if na == 0.0 || nb == 0.0 {
        return -1.0;
    }
    (dot / (na.sqrt() * nb.sqrt())) as f32
}

pub fn normalized(v: &[f32]) -> Vec<f32> {
    let n = v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    if n == 0.0 {
        return v.to_vec();
    }
    v.iter().map(|x| (*x as f64 / n) as f32).collect()
}

/// Décode une portion d'un média en PCM mono 16 kHz flottant via ffmpeg.
pub fn decode_pcm_f32(
    ffmpeg: &Path,
    media: &Path,
    start_ms: u64,
    duration_ms: Option<u64>,
) -> Result<Vec<f32>> {
    let mut cmd = Command::new(ffmpeg);
    suppress_child_console(&mut cmd);
    cmd.args([
        "-nostdin",
        "-v",
        "error",
        "-protocol_whitelist",
        LOCAL_PROTOCOLS,
        "-ss",
        &format!("{:.3}", start_ms as f64 / 1000.0),
        "-i",
    ])
    .arg(media);
    if let Some(d) = duration_ms {
        cmd.args(["-t", &format!("{:.3}", d as f64 / 1000.0)]);
    }
    cmd.args([
        "-vn",
        "-ac",
        "1",
        "-ar",
        &SAMPLE_RATE.to_string(),
        "-f",
        "f32le",
        "-acodec",
        "pcm_f32le",
        "-",
    ]);
    let out = cmd
        .output()
        .map_err(|e| DiarizationError::Decode(e.to_string()))?;
    if !out.status.success() {
        return Err(DiarizationError::Decode(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    if out.stdout.len() % 4 != 0 {
        return Err(DiarizationError::Decode("flux PCM tronqué".into()));
    }
    Ok(out
        .stdout
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect())
}

/// Attribue à chaque segment transcrit `(début, fin)` le locuteur dont les
/// tours le recouvrent le plus longtemps ; `None` sans recouvrement réel.
pub fn assign_speakers(segments: &[(u64, u64)], turns: &[SpeakerTurn]) -> Vec<Option<String>> {
    segments
        .iter()
        .map(|&(s, e)| {
            let mut totals: Vec<(&str, u64)> = Vec::new();
            for t in turns {
                let ov = e.min(t.end_ms).saturating_sub(s.max(t.start_ms));
                if ov == 0 {
                    continue;
                }
                match totals.iter_mut().find(|(id, _)| *id == t.speaker_id) {
                    Some(entry) => entry.1 += ov,
                    None => totals.push((&t.speaker_id, ov)),
                }
            }
            // Premier maximum rencontré : déterministe.
            let mut best: Option<(&str, u64)> = None;
            for (id, ov) in totals {
                if best.is_none_or(|(_, b)| ov > b) {
                    best = Some((id, ov));
                }
            }
            best.map(|(id, _)| id.to_string())
        })
        .collect()
}

// ---------------------------------------------------------------------------
// FFI sherpa-onnx (disposition vérifiée contre c-api.h v1.13.8)
// ---------------------------------------------------------------------------

#[repr(C)]
struct CEmbeddingConfig {
    model: *const c_char,
    num_threads: i32,
    debug: i32,
    provider: *const c_char,
}
#[repr(C)]
struct CPyannoteConfig {
    model: *const c_char,
    window_shift_ratio: f32,
}
#[repr(C)]
struct CSegmentationConfig {
    pyannote: CPyannoteConfig,
    num_threads: i32,
    debug: i32,
    provider: *const c_char,
}
#[repr(C)]
struct CClusteringConfig {
    num_clusters: i32,
    threshold: f32,
    compute_confidence: i32,
}
#[repr(C)]
struct CDiarizationConfig {
    segmentation: CSegmentationConfig,
    embedding: CEmbeddingConfig,
    clustering: CClusteringConfig,
    min_duration_on: f32,
    min_duration_off: f32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CSegment {
    start: f32,
    end: f32,
    speaker: i32,
    confidence: f32,
}

type Opaque = c_void;

#[allow(non_snake_case)]
struct Api {
    handle: *mut c_void,
    GetVersionStr: unsafe extern "C" fn() -> *const c_char,
    CreateSD: unsafe extern "C" fn(*const CDiarizationConfig) -> *const Opaque,
    DestroySD: unsafe extern "C" fn(*const Opaque),
    SDGetSampleRate: unsafe extern "C" fn(*const Opaque) -> i32,
    SDProcess: unsafe extern "C" fn(*const Opaque, *const f32, i32) -> *const Opaque,
    ResultNumSegments: unsafe extern "C" fn(*const Opaque) -> i32,
    ResultSorted: unsafe extern "C" fn(*const Opaque) -> *const CSegment,
    DestroySegments: unsafe extern "C" fn(*const CSegment),
    DestroyResult: unsafe extern "C" fn(*const Opaque),
    CreateExtractor: unsafe extern "C" fn(*const CEmbeddingConfig) -> *const Opaque,
    DestroyExtractor: unsafe extern "C" fn(*const Opaque),
    ExtractorDim: unsafe extern "C" fn(*const Opaque) -> i32,
    ExtractorCreateStream: unsafe extern "C" fn(*const Opaque) -> *const Opaque,
    AcceptWaveform: unsafe extern "C" fn(*const Opaque, i32, *const f32, i32),
    InputFinished: unsafe extern "C" fn(*const Opaque),
    ExtractorIsReady: unsafe extern "C" fn(*const Opaque, *const Opaque) -> i32,
    ExtractorCompute: unsafe extern "C" fn(*const Opaque, *const Opaque) -> *const f32,
    DestroyEmbedding: unsafe extern "C" fn(*const f32),
    DestroyStream: unsafe extern "C" fn(*const Opaque),
}

#[cfg(unix)]
mod dl {
    use super::*;
    const RTLD_NOW: c_int = 2;
    const RTLD_LOCAL: c_int = 0;
    extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlerror() -> *mut c_char;
        fn dlclose(handle: *mut c_void) -> c_int;
    }
    fn last_error() -> String {
        // SAFETY: dlerror renvoie NULL ou une chaîne C valide.
        unsafe {
            let e = dlerror();
            if e.is_null() {
                "erreur inconnue".into()
            } else {
                CStr::from_ptr(e).to_string_lossy().into_owned()
            }
        }
    }
    pub fn open(path: &Path) -> Result<*mut c_void> {
        use std::os::unix::ffi::OsStrExt;
        let c = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| DiarizationError::Library("chemin invalide".into()))?;
        // SAFETY: chemin C valide, drapeaux standards.
        let h = unsafe { dlopen(c.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
        if h.is_null() {
            return Err(DiarizationError::Library(last_error()));
        }
        Ok(h)
    }
    pub fn sym(h: *mut c_void, name: &str) -> Result<*mut c_void> {
        let c = CString::new(name).expect("nom de symbole");
        // SAFETY: poignée valide issue de dlopen.
        let p = unsafe { dlsym(h, c.as_ptr()) };
        if p.is_null() {
            return Err(DiarizationError::Library(format!(
                "symbole absent : {name}"
            )));
        }
        Ok(p)
    }
    pub fn close(h: *mut c_void) {
        // SAFETY: poignée valide, fermée une seule fois.
        unsafe {
            dlclose(h);
        }
    }
}

#[cfg(windows)]
#[path = "diarization_win.rs"]
mod dl;

#[cfg(not(any(unix, windows)))]
mod dl {
    use super::*;
    pub fn open(_: &Path) -> Result<*mut c_void> {
        Err(DiarizationError::Library(
            "chargement dynamique non pris en charge sur cette plateforme".into(),
        ))
    }
    pub fn sym(_: *mut c_void, _: &str) -> Result<*mut c_void> {
        unreachable!()
    }
    pub fn close(_: *mut c_void) {}
}

impl Api {
    // Le type cible de chaque transmute est fixé par le champ de `Api`, dont
    // la signature est recopiée de c-api.h.
    #[allow(clippy::missing_transmute_annotations)]
    fn load(path: &Path) -> Result<Self> {
        let handle = dl::open(path)?;
        macro_rules! f {
            ($name:literal) => {{
                match dl::sym(handle, concat!("SherpaOnnx", $name)) {
                    // SAFETY: signature recopiée depuis c-api.h pour la version vérifiée.
                    Ok(p) => unsafe { std::mem::transmute::<*mut c_void, _>(p) },
                    Err(e) => {
                        dl::close(handle);
                        return Err(e);
                    }
                }
            }};
        }
        let api = Api {
            handle,
            GetVersionStr: f!("GetVersionStr"),
            CreateSD: f!("CreateOfflineSpeakerDiarization"),
            DestroySD: f!("DestroyOfflineSpeakerDiarization"),
            SDGetSampleRate: f!("OfflineSpeakerDiarizationGetSampleRate"),
            SDProcess: f!("OfflineSpeakerDiarizationProcess"),
            ResultNumSegments: f!("OfflineSpeakerDiarizationResultGetNumSegments"),
            ResultSorted: f!("OfflineSpeakerDiarizationResultSortByStartTime"),
            DestroySegments: f!("OfflineSpeakerDiarizationDestroySegment"),
            DestroyResult: f!("OfflineSpeakerDiarizationDestroyResult"),
            CreateExtractor: f!("CreateSpeakerEmbeddingExtractor"),
            DestroyExtractor: f!("DestroySpeakerEmbeddingExtractor"),
            ExtractorDim: f!("SpeakerEmbeddingExtractorDim"),
            ExtractorCreateStream: f!("SpeakerEmbeddingExtractorCreateStream"),
            AcceptWaveform: f!("OnlineStreamAcceptWaveform"),
            InputFinished: f!("OnlineStreamInputFinished"),
            ExtractorIsReady: f!("SpeakerEmbeddingExtractorIsReady"),
            ExtractorCompute: f!("SpeakerEmbeddingExtractorComputeEmbedding"),
            DestroyEmbedding: f!("SpeakerEmbeddingExtractorDestroyEmbedding"),
            DestroyStream: f!("DestroyOnlineStream"),
        };
        Ok(api)
    }

    fn version(&self) -> String {
        // SAFETY: renvoie une chaîne statique.
        unsafe {
            let p = (self.GetVersionStr)();
            if p.is_null() {
                String::new()
            } else {
                CStr::from_ptr(p).to_string_lossy().into_owned()
            }
        }
    }
}

impl Drop for Api {
    fn drop(&mut self) {
        dl::close(self.handle);
    }
}

fn cpath(p: &Path) -> Result<CString> {
    CString::new(p.to_string_lossy().as_bytes())
        .map_err(|_| DiarizationError::InvalidConfig("chemin avec octet nul".into()))
}

/// Moteur natif chargé. Non partageable entre fils (`!Sync`) ; peut être
/// déplacé dans un fil de travail.
pub struct Diarizer {
    // Ordre de destruction : poignées natives avant la bibliothèque.
    sd: *const Opaque,
    extractor: *const Opaque,
    dim: usize,
    version: String,
    config: DiarizationConfig,
    api: Api,
}

// SAFETY: les objets sherpa-onnx ne sont utilisés que par un seul fil à la
// fois (`&mut self`/`&self` sans `Sync`) ; ils ne dépendent pas du fil créateur.
unsafe impl Send for Diarizer {}

impl Diarizer {
    pub fn new(config: DiarizationConfig) -> Result<Self> {
        config.validate()?;
        let api = Api::load(&config.sherpa_library)?;
        let version = api.version();
        if !SUPPORTED_SHERPA_PREFIXES
            .iter()
            .any(|p| version.starts_with(p))
        {
            return Err(DiarizationError::UnsupportedVersion(version));
        }
        let seg_model = cpath(&config.segmentation_model)?;
        let emb_model = cpath(&config.embedding_model)?;
        let cpu = CString::new("cpu").unwrap();
        let emb_cfg = CEmbeddingConfig {
            model: emb_model.as_ptr(),
            num_threads: config.num_threads,
            debug: 0,
            provider: cpu.as_ptr(),
        };
        let sd_cfg = CDiarizationConfig {
            segmentation: CSegmentationConfig {
                pyannote: CPyannoteConfig {
                    model: seg_model.as_ptr(),
                    window_shift_ratio: 0.0,
                },
                num_threads: config.num_threads,
                debug: 0,
                provider: cpu.as_ptr(),
            },
            embedding: CEmbeddingConfig {
                model: emb_model.as_ptr(),
                num_threads: config.num_threads,
                debug: 0,
                provider: cpu.as_ptr(),
            },
            clustering: CClusteringConfig {
                num_clusters: config.num_speakers,
                threshold: config.clustering_threshold,
                compute_confidence: 0,
            },
            min_duration_on: config.min_duration_on_s,
            min_duration_off: config.min_duration_off_s,
        };
        // SAFETY: structures conformes à c-api.h ; chaînes vivantes pendant l'appel
        // (sherpa-onnx copie la configuration).
        let sd = unsafe { (api.CreateSD)(&sd_cfg) };
        if sd.is_null() {
            return Err(DiarizationError::InitFailed);
        }
        // SAFETY: idem.
        let extractor = unsafe { (api.CreateExtractor)(&emb_cfg) };
        if extractor.is_null() {
            // SAFETY: sd valide.
            unsafe { (api.DestroySD)(sd) };
            return Err(DiarizationError::InitFailed);
        }
        // SAFETY: poignées valides.
        let (rate, dim) = unsafe { ((api.SDGetSampleRate)(sd), (api.ExtractorDim)(extractor)) };
        let this = Self {
            sd,
            extractor,
            dim: dim.max(0) as usize,
            version,
            config,
            api,
        };
        if rate != SAMPLE_RATE as i32 || this.dim == 0 {
            return Err(DiarizationError::InvalidConfig(format!(
                "fréquence {rate} Hz, dimension {dim}"
            )));
        }
        Ok(this)
    }

    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn embedding_dim(&self) -> usize {
        self.dim
    }
    pub fn config(&self) -> &DiarizationConfig {
        &self.config
    }

    /// Diarisation d'un signal mono 16 kHz, étiquettes locales à ce signal.
    pub fn diarize_samples(&self, samples: &[f32]) -> Result<Vec<LocalTurn>> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }
        let n = i32::try_from(samples.len())
            .map_err(|_| DiarizationError::Inference("tranche trop longue".into()))?;
        // SAFETY: tampon valide de n échantillons ; résultat libéré ci-dessous.
        unsafe {
            let result = (self.api.SDProcess)(self.sd, samples.as_ptr(), n);
            if result.is_null() {
                return Err(DiarizationError::Inference("résultat vide".into()));
            }
            let count = (self.api.ResultNumSegments)(result).max(0) as usize;
            let mut turns = Vec::with_capacity(count);
            if count > 0 {
                let segs = (self.api.ResultSorted)(result);
                if !segs.is_null() {
                    for s in std::slice::from_raw_parts(segs, count) {
                        turns.push(LocalTurn {
                            start_s: s.start,
                            end_s: s.end,
                            local_speaker: s.speaker,
                        });
                    }
                    (self.api.DestroySegments)(segs);
                }
            }
            (self.api.DestroyResult)(result);
            Ok(turns)
        }
    }

    /// Empreinte vocale normalisée d'un extrait mono 16 kHz.
    pub fn embed(&self, samples: &[f32]) -> Result<Vec<f32>> {
        let n = i32::try_from(samples.len())
            .map_err(|_| DiarizationError::Inference("extrait trop long".into()))?;
        // SAFETY: flux créé puis détruit ici ; vecteur copié avant libération.
        unsafe {
            let stream = (self.api.ExtractorCreateStream)(self.extractor);
            if stream.is_null() {
                return Err(DiarizationError::Inference("flux d'empreinte".into()));
            }
            (self.api.AcceptWaveform)(stream, SAMPLE_RATE as i32, samples.as_ptr(), n);
            (self.api.InputFinished)(stream);
            if (self.api.ExtractorIsReady)(self.extractor, stream) == 0 {
                (self.api.DestroyStream)(stream);
                return Err(DiarizationError::Inference(
                    "extrait trop court pour une empreinte".into(),
                ));
            }
            let v = (self.api.ExtractorCompute)(self.extractor, stream);
            if v.is_null() {
                (self.api.DestroyStream)(stream);
                return Err(DiarizationError::Inference("empreinte vide".into()));
            }
            let out = std::slice::from_raw_parts(v, self.dim).to_vec();
            (self.api.DestroyEmbedding)(v);
            (self.api.DestroyStream)(stream);
            if out.iter().any(|x| !x.is_finite()) {
                return Err(DiarizationError::Inference("empreinte non finie".into()));
            }
            Ok(normalized(&out))
        }
    }

    /// Diarise une tranche (`samples` commence à `chunk_start_ms` dans le
    /// média) et rattache ses locuteurs au registre global. Une tranche déjà
    /// enregistrée comme traitée n'enrichit plus les empreintes globales.
    pub fn diarize_chunk(
        &self,
        registry: &mut SpeakerRegistry,
        chunk_index: usize,
        chunk_start_ms: u64,
        samples: &[f32],
    ) -> Result<Vec<SpeakerTurn>> {
        let local = self.diarize_samples(samples)?;
        let mut labels: Vec<i32> = local.iter().map(|t| t.local_speaker).collect();
        labels.sort_unstable();
        labels.dedup();

        let to_idx =
            |s: f32| ((s.max(0.0) as f64 * SAMPLE_RATE as f64) as usize).min(samples.len());
        let mut embeddable: Vec<(i32, Vec<f32>, u64)> = Vec::new();
        for &label in &labels {
            let own: Vec<&LocalTurn> = local.iter().filter(|t| t.local_speaker == label).collect();
            let others: Vec<&LocalTurn> =
                local.iter().filter(|t| t.local_speaker != label).collect();
            let max = (self.config.max_embedding_ms * SAMPLE_RATE as u64 / 1000) as usize;
            let min = (self.config.min_embedding_ms * SAMPLE_RATE as u64 / 1000) as usize;
            // Parole propre : on retire les zones de chevauchement.
            let mut clean = Vec::new();
            for t in &own {
                let (a, b) = (to_idx(t.start_s), to_idx(t.end_s));
                for (i, &sample) in samples.iter().enumerate().take(b).skip(a) {
                    let ts = i as f32 / SAMPLE_RATE as f32;
                    if !others.iter().any(|o| ts >= o.start_s && ts < o.end_s) {
                        clean.push(sample);
                    }
                    if clean.len() >= max {
                        break;
                    }
                }
            }
            if clean.len() < min {
                clean.clear();
                for t in &own {
                    clean.extend_from_slice(&samples[to_idx(t.start_s)..to_idx(t.end_s)]);
                }
                clean.truncate(max);
            }
            if clean.len() < min {
                continue; // trop court : laissé sans locuteur plutôt que deviné
            }
            let ms = clean.len() as u64 * 1000 / SAMPLE_RATE as u64;
            match self.embed(&clean) {
                Ok(e) => embeddable.push((label, e, ms)),
                Err(DiarizationError::Inference(_)) => continue,
                Err(e) => return Err(e),
            }
        }

        let already = registry.processed_chunks.contains(&chunk_index);
        let ids: Vec<String> = if already {
            // Reprise : appariement sans mise à jour des empreintes.
            let mut snapshot = registry.clone();
            let ids = snapshot.match_embeddings(
                &embeddable
                    .iter()
                    .map(|(_, e, ms)| (e.clone(), *ms))
                    .collect::<Vec<_>>(),
                self.config.match_threshold,
            );
            if snapshot.speakers.len() > registry.speakers.len() {
                *registry = SpeakerRegistry {
                    processed_chunks: registry.processed_chunks.clone(),
                    ..snapshot
                };
            }
            ids
        } else {
            let ids = registry.match_embeddings(
                &embeddable
                    .iter()
                    .map(|(_, e, ms)| (e.clone(), *ms))
                    .collect::<Vec<_>>(),
                self.config.match_threshold,
            );
            registry.processed_chunks.push(chunk_index);
            ids
        };

        let mut out: Vec<SpeakerTurn> = local
            .iter()
            .filter_map(|t| {
                let pos = embeddable
                    .iter()
                    .position(|(l, _, _)| *l == t.local_speaker)?;
                let start_ms = chunk_start_ms + (t.start_s.max(0.0) as f64 * 1000.0).round() as u64;
                let end_ms = chunk_start_ms + (t.end_s.max(0.0) as f64 * 1000.0).round() as u64;
                (end_ms > start_ms).then(|| SpeakerTurn {
                    start_ms,
                    end_ms,
                    speaker_id: ids[pos].clone(),
                })
            })
            .collect();
        out.sort_by_key(|t| (t.start_ms, t.end_ms));
        Ok(out)
    }

    /// Diarise un média entier par tranches consécutives de `chunk_ms`.
    pub fn diarize_media_chunked(
        &self,
        ffmpeg: &Path,
        media: &Path,
        duration_ms: u64,
        chunk_ms: u64,
        registry: &mut SpeakerRegistry,
    ) -> Result<Vec<SpeakerTurn>> {
        if chunk_ms == 0 {
            return Err(DiarizationError::InvalidConfig("tranche nulle".into()));
        }
        let mut all = Vec::new();
        let mut start = 0;
        let mut index = 0;
        while start < duration_ms {
            let len = chunk_ms.min(duration_ms - start);
            let pcm = decode_pcm_f32(ffmpeg, media, start, Some(len))?;
            all.extend(self.diarize_chunk(registry, index, start, &pcm)?);
            start += len;
            index += 1;
        }
        Ok(all)
    }
}

impl Drop for Diarizer {
    fn drop(&mut self) {
        // SAFETY: poignées créées par cette instance, détruites une fois,
        // avant la fermeture de la bibliothèque (champ `api` détruit ensuite).
        unsafe {
            (self.api.DestroyExtractor)(self.extractor);
            (self.api.DestroySD)(self.sd);
        }
    }
}
