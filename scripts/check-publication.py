#!/usr/bin/env python3
"""Check a source publication without printing file contents or credentials."""
import argparse
import os
import pathlib
import re
import runpy
import subprocess
from urllib.parse import unquote, urlsplit


IGNORED = {'.git', 'node_modules', 'target', 'dist', '__pycache__', '.pytest_cache'}
PATTERNS = list(runpy.run_path(str(pathlib.Path(__file__).with_name('check-staged-secrets.py')), run_name='publication_checker')['PATTERNS'].values())
PATTERNS += [re.compile(rb'github_pat_[A-Za-z0-9_]{20,}'),
             re.compile(rb'gh[sur]_[A-Za-z0-9]{20,}'),
             re.compile(rb'(?:AKIA|ASIA)[0-9A-Z]{16}')]
PRIVATE = re.compile(rb'/srv/hermes-(?:data|cold)/|hermes\.tail[\w.-]*|github\.com/TomassonJW/parole(?:[\s"/]|$)' + rb'|\.hermes/' + rb'profile/')


def forbidden(name):
    name = name.lower()
    return (name in {'auth.json', 'vault', '.hermes', '.worktrees'}
            or name.startswith('.env')
            or name.endswith(('.pem', '.key', '.p12', '.gguf', '.onnx', '.wav', '.mp3', '.mp4', '.m4a')))


def scan(root):
    """Do not enter protected directories or open forbidden filenames."""
    findings = []
    tracked_directories = set()
    # Generated caches may be omitted only when Git is not publishing their contents.
    try:
        probe = subprocess.run(['git', '-C', str(root), 'rev-parse', '--show-toplevel'],
                               capture_output=True, text=True, check=False)
        if probe.returncode == 0 and pathlib.Path(probe.stdout.strip()).resolve() == root.resolve():
            index = subprocess.run(['git', '-C', str(root), 'ls-files', '-z'],
                                   capture_output=True, check=False)
            if index.returncode:
                findings.append((pathlib.Path('.'), 'git index unreadable'))
            else:
                for name in index.stdout.decode('utf-8', errors='strict').split('\0'):
                    if name:
                        tracked_directories.update(pathlib.Path(name).parents)
        elif (root / '.git').exists():
            findings.append((pathlib.Path('.'), 'git index unreadable'))
    except (OSError, UnicodeError):
        if (root / '.git').exists():
            findings.append((pathlib.Path('.'), 'git index unreadable'))

    def directory_error(error):
        path = pathlib.Path(error.filename) if error.filename else root
        findings.append((path.relative_to(root), 'unreadable directory'))

    for current, directories, files in os.walk(root, followlinks=False, onerror=directory_error):
        for name in list(directories):
            path = pathlib.Path(current) / name
            if name == '.git':
                directories.remove(name)
            elif path.is_symlink():
                findings.append((path.relative_to(root), 'symbolic link'))
                directories.remove(name)
            elif name in IGNORED and path.relative_to(root) not in tracked_directories:
                directories.remove(name)
            elif forbidden(name):
                findings.append((path.relative_to(root), 'forbidden name'))
                directories.remove(name)
        for name in files:
            path = pathlib.Path(current) / name
            if name == '.git':
                continue  # Linked worktree metadata is not a source publication.
            if forbidden(name):
                findings.append((path.relative_to(root), 'forbidden name'))
                continue
            if path.is_symlink():
                findings.append((path.relative_to(root), 'symbolic link'))
                continue
            if not path.is_file():
                findings.append((path.relative_to(root), 'non-regular file'))
                continue
            if path.stat().st_size >= 10_000_000:
                findings.append((path.relative_to(root), 'oversized file'))
                continue
            try:
                data = path.read_bytes()
            except OSError:
                findings.append((path.relative_to(root), 'unreadable file'))
                continue
            if any(pattern.search(data) for pattern in PATTERNS):
                findings.append((path.relative_to(root), 'credential pattern'))
            if PRIVATE.search(data):
                findings.append((path.relative_to(root), 'private reference'))
            if path.suffix.lower() == '.md':
                text = re.sub(r'(?ms)^```[^\n]*\n.*?^```[^\n]*(?:\n|$)', '', data.decode('utf-8', errors='replace'))
                for target in re.findall(r'!?\[[^\]]*\]\(([^)\s]+)(?:\s+[^)]*)?\)', text):
                    url = urlsplit(target)
                    if url.scheme or url.netloc or not url.path:
                        continue
                    resolved = (path.parent / unquote(url.path)).resolve()
                    if not resolved.is_relative_to(root.resolve()) or not resolved.is_file():
                        findings.append((path.relative_to(root), 'missing local link'))
    return findings


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=pathlib.Path, default=pathlib.Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    if not args.root.is_dir():
        print('BLOCK: repository directory absent')
        return 2
    findings = scan(args.root)
    for path, category in findings:
        print(f'BLOCK: {category}: {path}')
    print(f'{"BLOCK" if findings else "PASS"}: publication tree; {len(findings)} finding(s)')
    return 1 if findings else 0


if __name__ == '__main__':
    raise SystemExit(main())
