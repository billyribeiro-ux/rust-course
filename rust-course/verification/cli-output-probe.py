#!/usr/bin/env python3
"""Verify actual example executables with normal and preclosed OS pipes."""
import argparse
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--target-dir', type=Path, required=True)
args = parser.parse_args()
cases = {
    'hello': b'Hello, Rust workshop!\n',
    'roundoff': b'0.1 + 0.2 = 0.30000000000000004\nequal to 0.3: false\n',
    'workshop_basics': b'quantity: 3 (binary 11)\nsubtotal cents: 375\ndelivery cents: 250\npack item 1\npack item 2\npack item 3\nchecks remaining: 2\nchecks remaining: 1\nready\n',
}
for name, expected in cases.items():
    executable = args.target_dir / 'debug' / 'examples' / name
    result = subprocess.run([str(executable)], capture_output=True, timeout=10)
    assert result.returncode == 0 and result.stdout == expected and not result.stderr, (name, result)
    print(f'{name}: normal output and exit passed')
    # Close the sole reader before starting the child; no head/scheduler race.
    reader, writer = os.pipe()
    os.close(reader)
    try:
        result = subprocess.run([str(executable)], stdout=writer, stderr=subprocess.PIPE, timeout=10)
        assert result.returncode == 1 and b'panicked' not in result.stderr and result.stderr, (name, result)
        print(f'{name}: closed stdout returned 1 without panic')
        result = subprocess.run([str(executable)], stdout=writer, stderr=writer, timeout=10)
        assert result.returncode == 1, (name, result)
        print(f'{name}: closed stdout plus stderr returned 1 without panic exit')
    finally:
        os.close(writer)
print('9 example output observations passed; no browser behavior tested.')
