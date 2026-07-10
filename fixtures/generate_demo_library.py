#!/usr/bin/env python3
"""Generate a realistic demo Sims 4 Mods library for safe testing.

Deterministic (seeded), standard library only. Creates a folder full of
plausible-looking files with KNOWN, DOCUMENTED issues so every app feature
can be validated against expected findings:

  * 3 exact-duplicate pairs (byte-identical content, different paths)
  * 1 zero-byte package
  * 1 .ts4script nested too deep (below the default depth limit of 1)
  * archives (.zip/.rar/.7z) the game would ignore
  * unsupported files, an empty directory, a Resource.cfg
  * a Disabled/ folder for testing scan exclusions

The generated files are NOT real mods — package files contain random bytes
behind a DBPF-like 4-byte magic so nothing here could ever be mistaken for
redistributed content. Do not put this folder inside a real Mods folder.

Usage:
    python3 fixtures/generate_demo_library.py [output_dir]
Default output: ./demo-library
"""

import random
import sys
from pathlib import Path

SEED = 20260710


def payload(rng: random.Random, size: int, magic: bytes = b"DBPF") -> bytes:
    body = bytes(rng.getrandbits(8) for _ in range(max(0, size - len(magic))))
    return magic + body


def main() -> None:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "demo-library")
    if out.exists() and any(out.iterdir()):
        print(f"Refusing to write into non-empty directory: {out}")
        sys.exit(1)
    out.mkdir(parents=True, exist_ok=True)
    rng = random.Random(SEED)
    findings: list[str] = []

    def write(rel: str, data: bytes) -> None:
        p = out / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)

    # Resource.cfg (default-style; the app treats it as config, never edits)
    write(
        "Resource.cfg",
        b"Priority 500\nPackedFile *.package\nPackedFile */*.package\n"
        b"PackedFile */*/*.package\nPackedFile */*/*/*.package\n"
        b"PackedFile */*/*/*/*.package\nPackedFile */*/*/*/*/*.package\n",
    )

    # --- Ordinary, healthy content ------------------------------------
    creators = ["PixelPetal", "CozyCarat", "MossMitten", "SundaySeam"]
    cas_kinds = [("Hair", 9), ("Clothing", 12), ("Accessories", 5), ("Makeup", 4)]
    for kind, count in cas_kinds:
        for i in range(count):
            creator = creators[i % len(creators)]
            size = rng.randint(40_000, 400_000)
            write(
                f"CAS/{kind}/{creator}/{creator.lower()}-{kind.lower()}-{i + 1:02}.package",
                payload(rng, size),
            )
    for i in range(8):
        creator = creators[(i + 1) % len(creators)]
        write(
            f"BuildBuy/Furniture/{creator}/{creator.lower()}-set-{i + 1:02}.package",
            payload(rng, rng.randint(60_000, 500_000)),
        )

    # A well-placed script mod (exactly one folder deep = fine)
    write("UI Cheats Extension/ui-cheats.ts4script", payload(rng, 90_000, b"PK\x03\x04"))
    write("UI Cheats Extension/ui-cheats.package", payload(rng, 120_000))
    findings.append("OK: UI Cheats Extension/ui-cheats.ts4script is 1 level deep (loadable)")

    # --- Deliberate issues ---------------------------------------------
    # 1) Exact duplicates: identical bytes, different paths
    dup_specs = [
        (
            "CAS/Hair/PixelPetal/pixelpetal-wavy-bob.package",
            "Downloads/pixelpetal-wavy-bob (1).package",
            180_000,
        ),
        (
            "BuildBuy/Decorations/cozycarat-wall-art.package",
            "BuildBuy/Decorations/old/cozycarat-wall-art.package",
            95_000,
        ),
        (
            "CAS/Clothing/SundaySeam/sundayseam-cardigan.package",
            "Unsorted/sundayseam-cardigan copy.package",
            230_000,
        ),
    ]
    for keep, extra, size in dup_specs:
        data = payload(rng, size)
        write(keep, data)
        write(extra, data)
        findings.append(f"DUPLICATE PAIR ({size:,} bytes): {keep}  ==  {extra}")

    # 2) Zero-byte package (failed download)
    write("CAS/Hair/broken-download.package", b"")
    findings.append("ZERO-BYTE: CAS/Hair/broken-download.package")

    # 3) Script nested too deep (2 levels; default limit is 1)
    write(
        "Gameplay/Deep/Nested/too-deep-tuner.ts4script",
        payload(rng, 45_000, b"PK\x03\x04"),
    )
    findings.append(
        "DEEP SCRIPT (won't load with default Resource.cfg): "
        "Gameplay/Deep/Nested/too-deep-tuner.ts4script"
    )

    # 4) Archives the game ignores
    write("Downloads/mossmitten-bundle.zip", payload(rng, 300_000, b"PK\x03\x04"))
    write("Downloads/holiday-pack.rar", payload(rng, 150_000, b"Rar!"))
    write("Downloads/poses.7z", payload(rng, 80_000, b"7z\xbc\xaf"))
    findings.append("ARCHIVES (invisible to the game): Downloads/*.zip|rar|7z")

    # 5) Unsupported / stray files
    write("notes.txt", b"remember to update MCCC after the next patch\n")
    write("CAS/preview.png", payload(rng, 20_000, b"\x89PNG"))
    write("random-export.xyz", payload(rng, 5_000, b"????"))
    findings.append("UNSUPPORTED: random-export.xyz (plus benign notes.txt, preview.png)")

    # 6) Empty directory
    (out / "BuildBuy" / "Lighting").mkdir(parents=True, exist_ok=True)
    findings.append("EMPTY DIR: BuildBuy/Lighting/")

    # 7) Exclusion-testing folder
    write("Disabled/parked-mod.package", payload(rng, 70_000))
    findings.append(
        "EXCLUSION TEST: add 'Disabled' to scan exclusions in Settings; "
        "parked-mod.package must NOT be marked missing afterwards"
    )

    total_files = sum(1 for p in out.rglob("*") if p.is_file())
    manifest = [
        "DEMO LIBRARY — EXPECTED FINDINGS",
        "=" * 40,
        f"Total files (incl. this manifest once written): {total_files + 1}",
        "",
        *findings,
        "",
        "This folder is generated test data (random bytes), not real mods.",
    ]
    write("MANIFEST.txt", ("\n".join(manifest) + "\n").encode())
    print(f"Demo library written to {out} ({total_files + 1} files).")
    print("Point onboarding at this folder for a safe first validation run.")


if __name__ == "__main__":
    main()
