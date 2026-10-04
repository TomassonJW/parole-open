//! Couture synchrone backend → enfant local → cache canonique, hors fil UI.
//! Le service ne fournit ni commande Tauri ni reprise automatique.
use crate::{Gliclass, ModelBundle, retained::RetainedClassification, worker_protocol};
use parole_core::{
    classification_process::{ManagedProcess, StopReason},
    topic_classification_access::{
        ClassificationLibrary, ClassificationRequest, ClassificationSnapshot, ClassificationWriter,
    },
    topic_classification_cache::EngineIdentity,
    topic_questions::CandidateChoice,
    topic_selection::SelectionThresholds,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const INVALID: &str = "Le service de classement local est indisponible ou non vérifiable.";
const FAILED: &str = "Le classement local n'a pas été conservé et vérifié.";
static NEXT_RUN: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Default)]
pub struct ServiceCancellation(Arc<AtomicBool>);
impl ServiceCancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    fn cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Choix locaux du backend ; ne jamais construire cette valeur avec des chemins de fenêtre.
pub struct WorkerService {
    executable: PathBuf,
    root: PathBuf,
    bundle: ModelBundle,
    budget: Duration,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Choice {
    Word { term: String },
    PossibleFolder { name: String },
    WordInPossibleFolder { term: String, folder: String },
}
impl From<&CandidateChoice> for Choice {
    fn from(value: &CandidateChoice) -> Self {
        match value {
            CandidateChoice::Word { term } => Self::Word { term: term.clone() },
            CandidateChoice::PossibleFolder { name } => Self::PossibleFolder { name: name.clone() },
            CandidateChoice::WordInPossibleFolder { term, folder } => Self::WordInPossibleFolder {
                term: term.clone(),
                folder: folder.clone(),
            },
        }
    }
}
impl From<Choice> for CandidateChoice {
    fn from(value: Choice) -> Self {
        match value {
            Choice::Word { term } => Self::Word { term },
            Choice::PossibleFolder { name } => Self::PossibleFolder { name },
            Choice::WordInPossibleFolder { term, folder } => {
                Self::WordInPossibleFolder { term, folder }
            }
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireWork {
    job_id: String,
    source_revision: String,
    target: usize,
    choices: Vec<Choice>,
    engine: EngineIdentity,
    threshold_bits: [u64; 2],
}
impl From<&ClassificationRequest> for WireWork {
    fn from(req: &ClassificationRequest) -> Self {
        Self {
            job_id: req.job_id.clone(),
            source_revision: req.source_revision.clone(),
            target: req.target,
            choices: req.choices.iter().map(Choice::from).collect(),
            engine: req.engine.clone(),
            threshold_bits: [
                req.thresholds.uncertain_from.to_bits(),
                req.thresholds.proposed_from.to_bits(),
            ],
        }
    }
}
impl From<WireWork> for ClassificationRequest {
    fn from(work: WireWork) -> Self {
        Self {
            job_id: work.job_id,
            source_revision: work.source_revision,
            target: work.target,
            choices: work
                .choices
                .into_iter()
                .map(CandidateChoice::from)
                .collect(),
            engine: work.engine,
            thresholds: SelectionThresholds {
                uncertain_from: f64::from_bits(work.threshold_bits[0]),
                proposed_from: f64::from_bits(work.threshold_bits[1]),
            },
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRequest {
    run_id: String,
    root: PathBuf,
    bundle: ModelBundle,
    work: WireWork,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ResultKind {
    Saved,
    Reused,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireReply {
    run_id: String,
    job_id: String,
    source_revision: String,
    target: usize,
    question_revision: String,
    engine_revision: String,
    kind: ResultKind,
}
fn safe_path(path: &Path) -> bool {
    path.is_absolute() && !path.components().any(|c| matches!(c, Component::ParentDir))
}
fn executable_digest(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|_| INVALID)?;
    let mut hash = Sha256::new();
    let mut chunk = [0u8; 8192];
    let mut total = 0usize;
    loop {
        let n = file.read(&mut chunk).map_err(|_| INVALID)?;
        if n == 0 {
            break;
        }
        total += n;
        if total > 256 * 1024 * 1024 {
            return Err(INVALID.into());
        }
        hash.update(&chunk[..n]);
    }
    if total == 0 {
        return Err(INVALID.into());
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn run_id() -> String {
    let clock = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{}-{clock}-{}",
        std::process::id(),
        NEXT_RUN.fetch_add(1, Ordering::Relaxed)
    )
}
impl WorkerService {
    pub fn new(
        executable: PathBuf,
        root: PathBuf,
        bundle: ModelBundle,
        budget: Duration,
    ) -> Result<Self, String> {
        if !safe_path(&executable) || !safe_path(&root) || budget.is_zero() {
            return Err(INVALID.into());
        }
        Gliclass::new(bundle.clone()).map_err(|_| INVALID)?;
        Ok(Self {
            executable,
            root,
            bundle,
            budget,
        })
    }
    /// Identité attendue pour le binaire choisi localement, sans lancer de modèle.
    pub fn expected_identity(&self) -> Result<EngineIdentity, String> {
        let mut expected = Gliclass::new(self.bundle.clone())
            .map_err(|_| INVALID)?
            .expected_cache_identity()
            .map_err(|_| INVALID)?;
        expected.engine_sha256 = executable_digest(&self.executable)?;
        Ok(expected)
    }
    /// Une consultation historique appelle `ClassificationLibrary::load`, pas cette méthode.
    /// Une erreur ne retourne jamais un score, une transcription ou un diagnostic enfant.
    pub fn classify(
        &self,
        request: &ClassificationRequest,
        cancel: &ServiceCancellation,
    ) -> Result<ClassificationSnapshot, String> {
        if cancel.cancelled() {
            return Err("Classement local annulé.".into());
        }
        if request.engine != self.expected_identity()? {
            return Err(INVALID.into());
        }
        let library = ClassificationLibrary::open(&self.root).map_err(|_| INVALID)?;
        let plan = library.prepare(request).map_err(|_| INVALID)?;
        // Le cache présent est lu sans ouvrir un enfant et sans consulter de modèle.
        let existing = library.load(request).map_err(|_| INVALID)?;
        if existing.record.is_some() {
            if cancel.cancelled() {
                return Err("Classement local annulé.".into());
            }
            return Ok(existing);
        }
        let wire = WireRequest {
            run_id: run_id(),
            root: self.root.clone(),
            bundle: self.bundle.clone(),
            work: WireWork::from(request),
        };
        // La sérialisation bornée précède le lancement : pas d'enfant sur une requête trop grande.
        worker_protocol::write_frame(&mut io::sink(), &wire).map_err(|_| INVALID)?;
        if cancel.cancelled() {
            return Err("Classement local annulé.".into());
        }
        let mut command = Command::new(&self.executable);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let expected_run_id = wire.run_id.clone();
        let started = Instant::now();
        let mut child = ManagedProcess::spawn(command, self.budget).map_err(|_| INVALID)?;
        let pipes = child.take_pipes();
        let (tx, rx) = mpsc::channel();
        let mut stdin = pipes.stdin.ok_or(INVALID)?;
        let send = tx.clone();
        thread::spawn(move || {
            let result = worker_protocol::write_frame(&mut stdin, &wire).is_ok();
            drop(stdin);
            let _ = send.send(Event::Sent(result));
        });
        let mut stdout = pipes.stdout.ok_or(INVALID)?;
        thread::spawn(move || {
            let reply: io::Result<WireReply> = worker_protocol::read_frame(&mut stdout);
            let reply = reply.and_then(|reply| {
                let mut trailing = [0];
                if stdout.read(&mut trailing)? != 0 {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                Ok(reply)
            });
            let _ = tx.send(Event::Reply(reply.ok()));
        });
        // Les diagnostics ne deviennent jamais un message utilisateur. Acquisition bornée,
        // puis fermeture du descripteur même en cas de sortie illimitée.
        if let Some(mut stderr) = pipes.stderr {
            thread::spawn(move || {
                let _ = io::copy(&mut stderr.by_ref().take(8193), &mut io::sink());
            });
        }
        let mut sent = None;
        let mut reply = None;
        loop {
            if cancel.cancelled() {
                child.cancellation().cancel();
            }
            if started.elapsed() >= self.budget {
                child.cancellation().cancel();
                return Err("Le délai du classement local est dépassé.".into());
            }
            let outcome = child.poll().map_err(|_| FAILED)?;
            while let Ok(event) = rx.try_recv() {
                match event {
                    Event::Sent(ok) => sent = Some(ok),
                    Event::Reply(value) => reply = Some(value),
                }
            }
            if let Some(outcome) = outcome {
                if cancel.cancelled() || outcome.stop_requested == Some(StopReason::Cancelled) {
                    return Err("Classement local annulé.".into());
                }
                if outcome.stop_requested == Some(StopReason::Deadline) {
                    return Err("Le délai du classement local est dépassé.".into());
                }
                if !outcome.status.success() {
                    return Err(FAILED.into());
                }
                if sent.is_some() && reply.is_some() {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        if sent != Some(true) {
            return Err(FAILED.into());
        }
        let response = reply.flatten().ok_or(FAILED)?;
        if !matches!(response.kind, ResultKind::Saved)
            || response.run_id.is_empty()
            || response.run_id != expected_run_id
        {
            return Err(FAILED.into());
        }
        let loaded = library.load(request).map_err(|_| FAILED)?;
        let record = loaded.record.as_ref().ok_or(FAILED)?;
        if response.job_id != request.job_id
            || response.source_revision != request.source_revision
            || response.target != request.target
            || response.engine_revision != plan.plan().engine_revision()
            || response.question_revision != plan.plan().question().request_revision
            || record.question() != plan.plan().question()
            || cancel.cancelled()
        {
            return Err(FAILED.into());
        }
        Ok(loaded)
    }
}
enum Event {
    Sent(bool),
    Reply(Option<WireReply>),
}

/// Entrée du binaire produit : une seule requête, aucun mode de simulation.
pub fn child_main() -> Result<(), String> {
    let request: WireRequest = worker_protocol::read_frame(&mut io::stdin()).map_err(|_| FAILED)?;
    if request.run_id.is_empty()
        || request.run_id.len() > 128
        || !request.run_id.is_ascii()
        || !safe_path(&request.root)
    {
        return Err(FAILED.into());
    }
    let work: ClassificationRequest = request.work.into();
    let mut engine = Gliclass::new(request.bundle).map_err(|_| FAILED)?;
    if engine.expected_cache_identity().map_err(|_| FAILED)? != work.engine {
        return Err(FAILED.into());
    }
    let writer = ClassificationWriter::open(&request.root).map_err(|_| FAILED)?;
    let result = engine
        .classify_retained(&writer, &work)
        .map_err(|_| FAILED)?;
    let kind = match result {
        RetainedClassification::Saved(_) => ResultKind::Saved,
        RetainedClassification::Reused(_) => ResultKind::Reused,
    };
    let snapshot = result.snapshot();
    let revision = snapshot
        .record
        .as_ref()
        .ok_or(FAILED)?
        .question()
        .request_revision
        .clone();
    let response = WireReply {
        run_id: request.run_id,
        job_id: snapshot.job_id.clone(),
        source_revision: snapshot.source_revision.clone(),
        target: snapshot.target,
        question_revision: revision,
        engine_revision: work.engine.revision().map_err(|_| FAILED)?,
        kind,
    };
    worker_protocol::write_frame(&mut io::stdout(), &response).map_err(|_| FAILED)?;
    io::stdout().flush().map_err(|_| FAILED)?;
    Ok(())
}
