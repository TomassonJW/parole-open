//! Diagnostic borné dans le vrai processus signé, sans média ni inférence.
//! N'accepte aucun chemin utilisateur et n'écrit pas dans les données de Parole.
use parole_core::diarization::{DiarizationConfig, Diarizer};
use std::path::{Path, PathBuf};

fn native_directory(executable: &Path) -> Result<PathBuf, String> {
    let macos = executable.parent().ok_or("Exécutable sans dossier")?;
    let contents = macos.parent().ok_or("Paquet sans Contents")?;
    if macos.file_name().is_none_or(|name| name != "MacOS")
        || contents.file_name().is_none_or(|name| name != "Contents")
    {
        return Err("Diagnostic réservé au paquet Parole.app".into());
    }
    Ok(contents.join("Resources/native"))
}

/// None laisse démarrer l'interface habituelle. Le diagnostic est explicite.
pub fn requested() -> Option<i32> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 || args[0] != "--diagnostic-voix" {
        return None;
    }
    let result = std::env::current_exe()
        .map_err(|error| error.to_string())
        .and_then(|exe| native_directory(&exe))
        .and_then(|native| {
            let config = DiarizationConfig::new(
                native.join("libsherpa-onnx-c-api.dylib"),
                native.join("segmentation.onnx"),
                native.join("embedding.onnx"),
            );
            let engine = Diarizer::new(config).map_err(|error| error.to_string())?;
            Ok(serde_json::json!({
                "status": "ok",
                "library_version": engine.version(),
                "embedding_dimensions": engine.embedding_dim(),
                "inference_performed": false,
                "user_media_accessed": false
            }))
        });
    Some(match result {
        Ok(report) => {
            println!("{report}");
            0
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_only_the_running_application_resources() {
        let path = Path::new("/Applications/Parole.app/Contents/MacOS/parole-desktop");
        assert_eq!(
            native_directory(path).unwrap(),
            PathBuf::from("/Applications/Parole.app/Contents/Resources/native")
        );
    }

    #[test]
    fn rejects_an_executable_outside_a_bundle() {
        assert!(native_directory(Path::new("/tmp/parole-desktop")).is_err());
        assert!(native_directory(Path::new("/tmp/Other/MacOS/parole-desktop")).is_err());
    }
}
