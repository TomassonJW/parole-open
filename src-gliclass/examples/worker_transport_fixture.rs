//! Programme hostile synthétique de transport, distinct du binaire produit.
//! Les modes canoniques sauvegardent un vrai résultat vide, jamais de score ni modèle.
use parole_core::{
    topic_classification_access::{
        ClassificationLibrary, ClassificationRequest, ClassificationWriter,
    },
    topic_selection::SelectionThresholds,
};
use parole_gliclass::{Gliclass, ModelBundle, retained::RetainedClassification};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
    thread,
    time::{Duration, Instant},
};
fn ready(root: &Path, data: Value) {
    let pending = root.join("fixture-ready.tmp");
    fs::write(&pending, serde_json::to_vec(&data).unwrap()).unwrap();
    fs::rename(pending, root.join("fixture-ready.json")).unwrap();
}
fn wait_release(root: &Path) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !root.join("fixture-release.txt").exists() {
        assert!(Instant::now() < deadline, "Barrière de test non libérée.");
        thread::sleep(Duration::from_millis(2));
    }
}
fn respond(output: &mut impl Write, response: &Value) {
    let raw = serde_json::to_vec(response).unwrap();
    output.write_all(&(raw.len() as u32).to_be_bytes()).unwrap();
    output.write_all(&raw).unwrap();
    output.flush().unwrap();
}
fn canonical_response(input: &Value, root: &Path, mode: &str) -> Value {
    assert!(input["work"]["choices"].as_array().unwrap().is_empty());
    let bundle: ModelBundle = serde_json::from_value(input["bundle"].clone()).unwrap();
    let request = ClassificationRequest {
        job_id: input["work"]["job_id"].as_str().unwrap().into(),
        source_revision: input["work"]["source_revision"].as_str().unwrap().into(),
        target: input["work"]["target"].as_u64().unwrap() as usize,
        choices: vec![],
        engine: serde_json::from_value(input["work"]["engine"].clone()).unwrap(),
        thresholds: SelectionThresholds {
            uncertain_from: f64::from_bits(input["work"]["threshold_bits"][0].as_u64().unwrap()),
            proposed_from: f64::from_bits(input["work"]["threshold_bits"][1].as_u64().unwrap()),
        },
    };
    let library = ClassificationLibrary::open(root).unwrap();
    let plan = library.prepare(&request).unwrap();
    let mut engine = Gliclass::new(bundle).unwrap();
    assert!(engine.expected_cache_identity().unwrap() == request.engine);
    let writer = ClassificationWriter::open(root).unwrap();
    let result = engine.classify_retained(&writer, &request).unwrap();
    assert!(matches!(result, RetainedClassification::Saved(_)));
    assert!(!engine.is_loaded() && engine.inference_count() == 0);
    let mut response = json!({
        "run_id": input["run_id"], "job_id": request.job_id,
        "source_revision": request.source_revision, "target": request.target,
        "question_revision": result.snapshot().record.as_ref().unwrap().question().request_revision,
        "engine_revision": request.engine.revision().unwrap(), "kind": "saved"
    });
    match mode {
        "canonical-wrong-run" => response["run_id"] = json!("different-run"),
        "canonical-wrong-job" => response["job_id"] = json!("67676767-6767-4676-8676-676767676767"),
        "canonical-wrong-source" => response["source_revision"] = json!("0".repeat(64)),
        "canonical-wrong-target" => response["target"] = json!(1),
        "canonical-wrong-question" => response["question_revision"] = json!("0".repeat(64)),
        "canonical-wrong-engine" => response["engine_revision"] = json!("0".repeat(64)),
        "canonical-extra-scores" => response["scores"] = json!([]),
        "canonical-stale-source" => {
            let path = root.join(&request.job_id).join("travail.json");
            let mut job: parole_core::Job =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            job.segments[0].text.push_str(" Source fictive modifiée.");
            fs::write(path, serde_json::to_vec(&job).unwrap()).unwrap();
        }
        "canonical-corrupt-cache" => {
            let path = root
                .join(&request.job_id)
                .join("topics-classified-v1")
                .join(format!("classification-{}.json", plan.plan().cache_key()));
            fs::write(path, b"{}").unwrap();
        }
        _ => {}
    }
    ready(
        root,
        json!({
            "stage": "canonical-ready", "pid": std::process::id(), "saved": true,
            "model_loaded": engine.is_loaded(), "inference_count": engine.inference_count()
        }),
    );
    response
}
fn main() {
    let mut header = [0u8; 4];
    if io::stdin().read_exact(&mut header).is_err() {
        return;
    }
    let len = u32::from_be_bytes(header) as usize;
    if !(1..=65536).contains(&len) {
        return;
    }
    let mut bytes = vec![0u8; len];
    if io::stdin().read_exact(&mut bytes).is_err() {
        return;
    }
    let Ok(input) = serde_json::from_slice::<Value>(&bytes) else {
        return;
    };
    let Some(root) = input["root"].as_str() else {
        return;
    };
    let root = Path::new(root);
    let Ok(mode) = fs::read_to_string(root.join("fixture-mode.txt")) else {
        return;
    };
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("fixture-starts.txt"))
        .unwrap()
        .write_all(b"started\n")
        .unwrap();
    if !mode.starts_with("canonical-") && mode != "blocked" {
        ready(
            root,
            json!({"stage": "transport-ready", "pid": std::process::id()}),
        );
    }
    let output = io::stdout();
    let mut output = output.lock();
    match mode.as_str() {
        "zero" => {}
        "blocked" => {
            output.write_all(&32u32.to_be_bytes()).unwrap();
            output.flush().unwrap();
            ready(
                root,
                json!({"stage": "blocked-ready", "pid": std::process::id()}),
            );
            wait_release(root);
        }
        "oversized" => {
            let _ = output.write_all(&65537u32.to_be_bytes());
        }
        "truncated" => {
            let _ = output.write_all(&32u32.to_be_bytes());
            let _ = output.write_all(b"{");
        }
        "stderr" => {
            let _ = io::stderr().write_all(&vec![b'X'; 65536]);
        }
        "canonical-valid"
        | "canonical-wrong-run"
        | "canonical-wrong-job"
        | "canonical-wrong-source"
        | "canonical-wrong-target"
        | "canonical-wrong-question"
        | "canonical-wrong-engine"
        | "canonical-extra-scores"
        | "canonical-stale-source"
        | "canonical-corrupt-cache"
        | "canonical-hold"
        | "canonical-no-reply"
        | "canonical-trailing" => {
            let response = canonical_response(&input, root, &mode);
            if mode == "canonical-hold" {
                wait_release(root);
            }
            if mode != "canonical-no-reply" {
                respond(&mut output, &response);
                if mode == "canonical-trailing" {
                    output.write_all(b"X").unwrap();
                    output.flush().unwrap();
                }
            }
        }
        "wrong-id" | "saved-no-cache" | "late" => {
            if mode == "late" {
                thread::sleep(Duration::from_millis(150));
            }
            let response = json!({"run_id": if mode == "wrong-id" { "other" } else { input["run_id"].as_str().unwrap_or("") },
                "job_id":input["work"]["job_id"], "source_revision":input["work"]["source_revision"],
                "target": input["work"]["target"], "question_revision":"fictional", "engine_revision":"fictional", "kind":"saved"});
            let raw = serde_json::to_vec(&response).unwrap();
            let _ = output.write_all(&(raw.len() as u32).to_be_bytes());
            let _ = output.write_all(&raw);
        }
        _ => std::process::exit(1),
    }
}
