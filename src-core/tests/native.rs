use parole_core::native::parse_whisper_json;
#[cfg(unix)]
use parole_core::native::probe_duration_ms;

#[cfg(unix)]
#[test]
fn sonde_limitee_aux_protocoles_locaux() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("enregistrement.wav");
    let probe = dir.path().join("sonde.sh");
    fs::write(&media, b"media").unwrap();
    fs::write(&probe, b"#!/bin/sh\ncase \" $* \" in *' -protocol_whitelist file,pipe '*) printf '12.5\\n';; *) exit 19;; esac\n").unwrap();
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(probe_duration_ms(&media, &probe).unwrap(), 12_500);
}

#[test]
fn parse_les_horodatages_natifs_sans_inventer_des_locuteurs() {
    let json = r#"{"result":{"language":"fr"},"transcription":[{"offsets":{"from":200,"to":1700},"text":" Bonjour."},{"offsets":{"from":1700,"to":2400},"text":" Merci."}]}"#;
    let segments = parse_whisper_json(json.as_bytes()).unwrap();
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].start_ms, 200);
    assert_eq!(segments[1].text, "Merci.");
    assert!(segments[0].speaker_id.is_none());
}

#[test]
fn rejette_un_horodatage_negatif() {
    let json = r#"{"transcription":[{"offsets":{"from":-1,"to":1000},"text":"oops"}]}"#;
    assert!(parse_whisper_json(json.as_bytes()).is_err());
}
