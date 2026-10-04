#!/usr/bin/env python3
"""Regression: load the voice engine inside the packaged, signed app itself."""
import json
import pathlib
import plistlib
import subprocess
import sys

root=pathlib.Path(__file__).resolve().parents[1]
app=root/'src-tauri/target/release/bundle/macos/Parole.app'
exe=app/'Contents/MacOS/parole-desktop'
out=root/'trial-evidence';out.mkdir(exist_ok=True)
result=subprocess.run([str(exe),'--diagnostic-voix'],capture_output=True,text=True,timeout=90)
record={'returncode':result.returncode,'stdout':result.stdout,'stderr':result.stderr,'probe':'actual signed Parole process, production Diarizer::new, no media and no inference'}
record['system_integrity_protection']=subprocess.run(['csrutil','status'],capture_output=True,text=True,check=False).stdout
record['version']=plistlib.loads((app/'Contents/Info.plist').read_bytes())['CFBundleShortVersionString']
for file in [exe]+list((app/'Contents/Resources/native').glob('*.dylib')):
    display=subprocess.run(['codesign','-dvvv',str(file)],capture_output=True,text=True,check=True)
    record[file.name]=display.stderr
(out/'signed-app-diarization.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record,indent=2))
if result.returncode:
    sys.exit(result.returncode)
report=json.loads(result.stdout)
assert report['status']=='ok' and report['library_version'].startswith('1.13.8')
assert report['embedding_dimensions']>0
assert report['inference_performed'] is False and report['user_media_accessed'] is False
subprocess.run(['codesign','--verify','--deep','--strict',str(app)],check=True)
subprocess.run([sys.executable,str(root/'scripts/check-macos-signing.py'),str(exe)],check=True)
expected_version=json.loads((root/'src-tauri/tauri.macos.conf.json').read_text())['version']
assert record['version']==expected_version
stdout=out/'launchservices-voice.stdout'
stderr=out/'launchservices-voice.stderr'
launch=subprocess.run(['open','-n','-W',str(app),'--stdout',str(stdout),'--stderr',str(stderr),'--args','--diagnostic-voix'],capture_output=True,text=True,timeout=90)
record['launchservices']={'returncode':launch.returncode,'launcher_error':launch.stderr,'stdout':stdout.read_text() if stdout.exists() else '', 'stderr':stderr.read_text() if stderr.exists() else ''}
(out/'signed-app-diarization.json').write_text(json.dumps(record,indent=2)+'\n')
assert launch.returncode==0,record['launchservices']
launched=json.loads(record['launchservices']['stdout'])
assert launched==report,record['launchservices']
print('Signed-app voice initialization PASS')
