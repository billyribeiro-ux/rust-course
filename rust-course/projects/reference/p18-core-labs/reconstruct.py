#!/usr/bin/env python3
"""Make one complete, standalone source checkpoint; refuse existing learner work."""
import argparse
import shutil
from pathlib import Path

package = Path(__file__).resolve().parent
course = package.parents[2]
parser = argparse.ArgumentParser()
parser.add_argument("lab", choices=("collections", "conversions", "interior"))
parser.add_argument("--name", required=True)
args = parser.parse_args()
if not args.name or Path(args.name).name != args.name or args.name in (".", ".."):
    raise SystemExit("Name must be one simple directory name")
destination = course / "learner-work" / args.name
try:
    destination.mkdir(parents=True, exist_ok=False)
except FileExistsError:
    raise SystemExit(f"Refusing to overwrite {destination}; choose another --name") from None
(destination / "src").mkdir()
(destination / "examples").mkdir()
(destination / "Cargo.toml").write_text(
    '[package]\nname = "core-rust-labs"\nversion = "0.1.0"\nedition = "2024"\n'
    'rust-version = "1.99"\npublish = false\n\n[workspace]\n'
)
(destination / "src/lib.rs").write_text(f"pub mod {args.lab};\n")
(destination / ".gitignore").write_text("/target/\n")
shutil.copyfile(package / "src" / f"{args.lab}.rs", destination / "src" / f"{args.lab}.rs")
shutil.copyfile(package / "examples" / f"{args.lab}.rs", destination / "examples" / f"{args.lab}.rs")
print(f"Created complete {args.lab} checkpoint: {destination}")
print(f"From that directory: mise exec -- cargo run --example {args.lab}")
print("Then: mise exec -- cargo test")
