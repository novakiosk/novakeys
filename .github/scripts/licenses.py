#!/usr/bin/env python3
"""Preserve notices from the locked Linux normal/build graph and Rust standard library."""
import json
from pathlib import Path
import shutil
import subprocess
import sys

metadata = json.loads(Path(sys.argv[1]).read_text())
destination = Path(sys.argv[2])
destination.mkdir(parents=True)
tree = subprocess.check_output([
    "cargo", "tree", "--locked", "--target", "x86_64-unknown-linux-gnu",
    "--edges", "normal,build", "--prefix", "none", "--format", "{p}"
], text=True)
selected = {(line.split()[0], line.split()[1].removeprefix("v"))
            for line in tree.splitlines()}
# These published crates omit the repository license. Text is pinned locally to
# their exact .cargo_vcs_info.json commits; future versions must be reviewed.
fallbacks = {
    ("gtk4-layer-shell", "0.8.1"): "7043a44adc4e31868e6f30a4f8c8279168417a94",
    ("gtk4-layer-shell-sys", "0.6.1"): "c52b9b12e6b1afa38dcb496ec821c55bb298da78",
}
index = []
for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
    if (package["name"], package["version"]) not in selected or package["id"] == metadata["resolve"]["root"]:
        continue
    root = Path(package["manifest_path"]).parent
    target = destination / f"{package['name']}-{package['version']}"
    notices = {p for p in root.rglob("*") if p.is_file() and
               p.name.upper().startswith(("LICENSE", "COPYING", "COPYRIGHT", "NOTICE", "AUTHORS"))}
    if package.get("license_file"):
        notices.add(root / package["license_file"])
    if not notices and (package["name"], package["version"]) in fallbacks:
        revision = fallbacks[(package["name"], package["version"])]
        actual = json.loads((root / ".cargo_vcs_info.json").read_text())["git"]["sha1"]
        if actual != revision:
            raise RuntimeError(f"Unexpected source revision for {package['name']}")
        target.mkdir(parents=True)
        shutil.copyfile(".github/licenses/gtk4-layer-shell-MIT.txt", target / "LICENSE")
    elif not notices:
        raise RuntimeError(f"Missing notices for {package['name']} {package['version']}")
    if package["name"] in ("wayland-client", "wayland-protocols", "wayland-protocols-misc"):
        notices.update(root.rglob("*.xml"))  # Protocol-specific permission blocks.
    for notice in sorted(notices):
        relative = notice.resolve().relative_to(root.resolve())
        output = target / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(notice, output)
    index.append({key: package.get(key) for key in ("name", "version", "license", "repository")})
(destination / "crates.json").write_text(json.dumps(index, indent=2) + "\n")
sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip())
rust = sysroot / "share/doc/rust"
shutil.copytree(rust / "licenses", destination / "rust/licenses")
shutil.copyfile(rust / "COPYRIGHT-library.html", destination / "rust/COPYRIGHT-library.html")
for name in ("LICENSE-MIT", "LICENSE-APACHE", "COPYRIGHT.html"):
    if (rust / name).is_file():
        shutil.copyfile(rust / name, destination / "rust" / name)
print(f"Packaged notices for {len(index)} Rust crates and the Rust standard library")
