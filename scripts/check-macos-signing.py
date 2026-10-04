#!/usr/bin/env python3
"""Reject hardened ad-hoc macOS packages that cannot satisfy Team ID validation."""
import argparse
import pathlib
import struct


def signing_profile(path):
    data = pathlib.Path(path).read_bytes()
    if len(data) < 32 or struct.unpack_from('<I', data)[0] != 0xFEEDFACF:
        raise ValueError('Expected thin Mach-O 64-bit executable')
    offset = 32
    for _ in range(struct.unpack_from('<I', data, 16)[0]):
        cmd, size = struct.unpack_from('<II', data, offset)
        if size < 8 or offset + size > len(data):
            raise ValueError('Invalid Mach-O command')
        if cmd == 0x1D:
            start, length = struct.unpack_from('<II', data, offset + 8)
            signature = data[start:start + length]
            magic, total, count = struct.unpack_from('>III', signature)
            if magic != 0xFADE0CC0 or total > len(signature):
                raise ValueError('Invalid signature container')
            for i in range(count):
                _, pos = struct.unpack_from('>II', signature, 12 + 8*i)
                magic, length = struct.unpack_from('>II', signature, pos)
                if magic == 0xFADE0C02:
                    blob = signature[pos:pos + length]
                    version, flags = struct.unpack_from('>II', blob, 8)
                    team_offset = struct.unpack_from('>I', blob, 48)[0] if version >= 0x20200 else 0
                    team = blob[team_offset:].split(b'\0', 1)[0].decode() if team_offset else None
                    return {'flags': flags, 'adhoc': bool(flags & 2), 'hardened': bool(flags & 0x10000), 'team': team}
        offset += size
    raise ValueError('Missing CodeDirectory')


def validate_profile(profile):
    if profile['adhoc'] and profile['hardened']:
        raise ValueError('Ad-hoc signature + hardened runtime: no Apple Team ID for bundled voice libraries')
    if not profile['adhoc'] and not profile['team']:
        raise ValueError('Non-ad-hoc signature without Team ID')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('executable', type=pathlib.Path)
    args = parser.parse_args()
    profile = signing_profile(args.executable)
    validate_profile(profile)
    print(f'Mac signature policy PASS: {profile}')
