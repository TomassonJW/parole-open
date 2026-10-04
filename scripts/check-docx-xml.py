#!/usr/bin/env python3
"""Validate every XML part of real Word exports with an independent parser."""
import argparse
import json
import pathlib
import xml.etree.ElementTree as ET
import zipfile


def validate(path):
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        required = {'[Content_Types].xml', '_rels/.rels', 'word/document.xml', 'word/styles.xml', 'word/_rels/document.xml.rels'}
        assert required.issubset(names), 'Missing Word XML part'
        assert len(names) == len(set(names)), 'Duplicate ZIP member'
        parts = []
        for item in archive.infolist():
            assert item.file_size <= 128 * 1024 * 1024, 'XML part exceeds safety limit'
            if item.filename.endswith(('.xml', '.rels')):
                ET.fromstring(archive.read(item))
                parts.append(item.filename)
        return {'file': pathlib.Path(path).name, 'xml_parts': parts, 'valid': True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('documents', nargs='+', type=pathlib.Path)
    args = parser.parse_args()
    results = [validate(path) for path in args.documents]
    print(json.dumps({'documents': len(results), 'checks': results}, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
