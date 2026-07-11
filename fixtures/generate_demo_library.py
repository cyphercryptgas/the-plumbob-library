#!/usr/bin/env python3
"""Generate a realistic demo Sims 4 Mods library for safe testing.

Deterministic (seeded), standard library only. Every .package file is a REAL
minimal DBPF (valid 96-byte header + resource index) so the app's package
indexing and conflict detection can be validated against known plants:

  * 3 exact-duplicate pairs (byte-identical DBPF, different paths) — these
    share every resource key but must appear in DUPLICATES, never Conflicts
  * 1 gameplay conflict between unrelated mods ("needs a look")
  * 1 gameplay overlap inside one mod's folder ("likely intentional")
  * 1 appearance-only overlap (shared thumbnail resource)
  * 1 corrupt package (truncated index → Library "Unreadable" filter)
  * 1 suspected-duplicate pair (same name, different content)
  * 1 zero-byte package, 1 too-deep script, archives, unsupported files,
    an empty directory, and a Disabled/ folder for exclusion testing

The generated files are NOT real mods — resource keys and padding are
random. Do not put this folder inside a real Mods folder.

Usage:
    python3 fixtures/generate_demo_library.py [output_dir]
Default output: ./demo-library
"""

import random
import struct
import sys
from pathlib import Path

SEED = 20260711

TUNING = 0x62E94D38
CASP = 0x034AEECB
SIMDATA = 0x545AC67A
THUMB = 0x3C1AF1F2

PLANT_TUNER = (TUNING, 0x00000000, 0xC0FFEE0000000001)
PLANT_COOLMOD = (TUNING, 0x00000000, 0xC0FFEE0000000002)
PLANT_THUMB = (THUMB, 0x00000000, 0xC0FFEE0000000003)


def dbpf(keys, rng: random.Random, pad_to: int = 0) -> bytes:
    """Byte-exact minimal DBPF 2.1 with full (flags=0) index entries."""
    out = bytearray(96)
    out[0:4] = b"DBPF"
    struct.pack_into("<I", out, 0x04, 2)
    struct.pack_into("<I", out, 0x08, 1)
    struct.pack_into("<I", out, 0x24, len(keys))
    struct.pack_into("<I", out, 0x2C, 4 + 32 * len(keys))
    struct.pack_into("<I", out, 0x3C, 3)
    struct.pack_into("<I", out, 0x40, 96)
    body = bytearray(struct.pack("<I", 0))  # flags: no constant fields
    for i, (t, g, inst) in enumerate(keys):
        body += struct.pack(
            "<IIII", t, g, (inst >> 32) & 0xFFFFFFFF, inst & 0xFFFFFFFF
        )
        body += struct.pack("<IIII", 0x1000 + i, 0x8000_0040, 0x40, 0x0001_5A42)
    blob = bytes(out) + bytes(body)
    if pad_to > len(blob):
        blob += bytes(rng.getrandbits(8) for _ in range(pad_to - len(blob)))
    return blob


def junk(rng: random.Random, size: int, magic: bytes) -> bytes:
    return magic + bytes(rng.getrandbits(8) for _ in range(max(0, size - len(magic))))


def main() -> None:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "demo-library")
    if out.exists() and any(out.iterdir()):
        print(f"Refusing to write into non-empty directory: {out}")
        sys.exit(1)
    out.mkdir(parents=True, exist_ok=True)
    rng = random.Random(SEED)
    findings: list[str] = []
    serial = [0]

    def write(rel: str, data: bytes) -> None:
        p = out / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)

    def package(rel: str, size: int, extra_keys=()) -> None:
        """A healthy package with unique random-ish keys plus any plants."""
        serial[0] += 1
        base = serial[0] << 20
        keys = [
            (CASP, 0, base + 1),
            (SIMDATA, 0, base + 2),
            (THUMB, 0x80000000, base + 3),
        ]
        keys.extend(extra_keys)
        write(rel, dbpf(keys, rng, pad_to=size))

    # Resource.cfg (default-style; the app treats it as config, never edits)
    write(
        "Resource.cfg",
        b"Priority 500\nPackedFile *.package\nPackedFile */*.package\n"
        b"PackedFile */*/*.package\nPackedFile */*/*/*.package\n"
        b"PackedFile */*/*/*/*.package\nPackedFile */*/*/*/*/*.package\n",
    )

    # --- Ordinary, healthy content ------------------------------------
    creators = ["PixelPetal", "CozyCarat", "MossMitten", "SundaySeam"]
    for kind, count in [("Hair", 9), ("Clothing", 12), ("Accessories", 5), ("Makeup", 4)]:
        for i in range(count):
            creator = creators[i % len(creators)]
            package(
                f"CAS/{kind}/{creator}/{creator.lower()}-{kind.lower()}-{i + 1:02}.package",
                rng.randint(40_000, 400_000),
            )
    for i in range(8):
        creator = creators[(i + 1) % len(creators)]
        package(
            f"BuildBuy/Furniture/{creator}/{creator.lower()}-set-{i + 1:02}.package",
            rng.randint(60_000, 500_000),
        )

    # A well-placed script mod (exactly one folder deep = fine)
    write("UI Cheats Extension/ui-cheats.ts4script", junk(rng, 90_000, b"PK\x03\x04"))
    package("UI Cheats Extension/ui-cheats.package", 120_000)
    findings.append("OK: UI Cheats Extension/ui-cheats.ts4script is 1 level deep (loadable)")

    # --- Exact duplicates: identical DBPF bytes, different paths ------
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
        serial[0] += 1
        base = serial[0] << 20
        data = dbpf([(CASP, 0, base + 1), (SIMDATA, 0, base + 2)], rng, pad_to=size)
        write(keep, data)
        write(extra, data)
        findings.append(f"DUPLICATE PAIR ({size:,} bytes): {keep}  ==  {extra}")
    findings.append(
        "NOT A CONFLICT: the three pairs above share every resource key but are "
        "byte-identical — they must appear in Duplicates and NEVER in Conflicts"
    )

    # --- Planted conflicts ---------------------------------------------
    package("TunerAlpha/tuner-alpha.package", 55_000, extra_keys=[PLANT_TUNER])
    package("TunerBravo/tuner-bravo.package", 62_000, extra_keys=[PLANT_TUNER])
    findings.append(
        "CONFLICT (gameplay, NEEDS A LOOK): TunerAlpha/tuner-alpha.package and "
        "TunerBravo/tuner-bravo.package share 1 tuning resource; "
        "tuner-bravo loads last (alphabetical) = presumptive winner"
    )

    package("CoolMod/coolmod-base.package", 70_000, extra_keys=[PLANT_COOLMOD])
    package("CoolMod/coolmod-addon.package", 45_000, extra_keys=[PLANT_COOLMOD])
    findings.append(
        "CONFLICT (gameplay, LIKELY INTENTIONAL — same folder): "
        "CoolMod/coolmod-base.package and CoolMod/coolmod-addon.package "
        "share 1 tuning resource; shown under 'probably fine'"
    )

    package("BuildBuy/Clutter/mossmitten-shelfie.package", 40_000, extra_keys=[PLANT_THUMB])
    package("CAS/Accessories/PixelPetal/pixelpetal-brooch.package", 38_000, extra_keys=[PLANT_THUMB])
    findings.append(
        "CONFLICT (APPEARANCE ONLY): BuildBuy/Clutter/mossmitten-shelfie.package "
        "and CAS/Accessories/PixelPetal/pixelpetal-brooch.package share 1 "
        "thumbnail resource; low severity, shown under 'probably fine'"
    )

    # --- Corrupt package (truncated index) -----------------------------
    serial[0] += 1
    healthy = dbpf([(CASP, 0, (serial[0] << 20) + 1)], rng)
    write("BuildBuy/Decorations/damaged-download.package", healthy[: 96 + 10])
    findings.append(
        "PARSE ERROR (truncated): BuildBuy/Decorations/damaged-download.package "
        "— appears under Library filter 'Unreadable'"
    )

    # --- Suspected duplicates: same name, different content ------------
    package("CAS/Hair/CozyCarat/cozycarat-braids.package", 150_000)
    package("Old CC/cozycarat-braids.package", 140_000)
    findings.append(
        "SUSPECTED DUPLICATE (same name, different content): "
        "CAS/Hair/CozyCarat/cozycarat-braids.package vs "
        "Old CC/cozycarat-braids.package — listed in the lower-confidence tier"
    )

    # --- Zero-byte package (failed download) ---------------------------
    write("CAS/Hair/broken-download.package", b"")
    findings.append("ZERO-BYTE: CAS/Hair/broken-download.package")

    # --- Script nested too deep (2 levels; default limit is 1) ---------
    write("Gameplay/Deep/Nested/too-deep-tuner.ts4script", junk(rng, 45_000, b"PK\x03\x04"))
    findings.append(
        "DEEP SCRIPT (won't load with default Resource.cfg): "
        "Gameplay/Deep/Nested/too-deep-tuner.ts4script"
    )

    # --- Archives the game ignores --------------------------------------
    write("Downloads/mossmitten-bundle.zip", junk(rng, 300_000, b"PK\x03\x04"))
    write("Downloads/holiday-pack.rar", junk(rng, 150_000, b"Rar!"))
    write("Downloads/poses.7z", junk(rng, 80_000, b"7z\xbc\xaf"))
    findings.append("ARCHIVES (invisible to the game): Downloads/*.zip|rar|7z")

    # --- Unsupported / stray files --------------------------------------
    write("notes.txt", b"remember to update MCCC after the next patch\n")
    write("CAS/preview.png", junk(rng, 20_000, b"\x89PNG"))
    write("random-export.xyz", junk(rng, 5_000, b"????"))
    findings.append("UNSUPPORTED: random-export.xyz (plus benign notes.txt, preview.png)")

    # --- Empty directory -------------------------------------------------
    (out / "BuildBuy" / "Lighting").mkdir(parents=True, exist_ok=True)
    findings.append("EMPTY DIR: BuildBuy/Lighting/")

    # --- Exclusion-testing folder ----------------------------------------
    package("Disabled/parked-mod.package", 70_000)
    findings.append(
        "EXCLUSION TEST: add 'Disabled' to scan exclusions in Settings; "
        "parked-mod.package must NOT be marked missing afterwards"
    )

    total_files = sum(1 for p in out.rglob("*") if p.is_file())
    manifest = [
        "DEMO LIBRARY — EXPECTED FINDINGS (Phase 2 edition)",
        "=" * 52,
        f"Total files (incl. this manifest once written): {total_files + 1}",
        "Every .package here is a real minimal DBPF; the parse pass should",
        "index all of them except the one planted corrupt file.",
        "",
        *findings,
        "",
        "This folder is generated test data (random resource keys), not real mods.",
    ]
    write("MANIFEST.txt", ("\n".join(manifest) + "\n").encode())
    print(f"Demo library written to {out} ({total_files + 1} files).")
    print("Point onboarding at this folder for a safe validation run.")


if __name__ == "__main__":
    main()
