#!/usr/bin/env python3
"""从旧单体 migrations.rs 提取 ledger 未收录的增量，便于合并到 incremental.rs。

用法:
  python3 scripts/port_legacy_migrations.py --legacy path/to/migrations.rs
  python3 scripts/port_legacy_migrations.py --check-duplicates
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MIG_DIR = ROOT / "backend" / "src" / "db" / "migrations"
SKIP_IDS = frozenset({"unique_migration_id_vX"})

ONCE_RE = re.compile(
    r'once_migration!\(\s*pool\s*,\s*done\s*,\s*"([^"]+)"\s*,',
    re.MULTILINE,
)
RUST_RE = re.compile(r'if\s*!done\.contains\("([^"]+)"\)')


def ids_in_text(text: str) -> list[str]:
    ordered: list[str] = []
    seen: set[str] = set()
    for m in ONCE_RE.finditer(text):
        i = m.group(1)
        if i in SKIP_IDS or i in seen:
            continue
        seen.add(i)
        ordered.append(i)
    for m in RUST_RE.finditer(text):
        i = m.group(1)
        if i in SKIP_IDS or i in seen:
            continue
        seen.add(i)
        ordered.append(i)
    return ordered


def collect_ledger_ids(mig_dir: Path = MIG_DIR) -> list[str]:
    ordered: list[str] = []
    seen: set[str] = set()
    for p in sorted((mig_dir / "ledger").glob("*.rs")):
        if p.name == "mod.rs":
            continue
        for i in ids_in_text(p.read_text()):
            if i not in seen:
                seen.add(i)
                ordered.append(i)
    return ordered


def collect_ledger_once_ids(mig_dir: Path = MIG_DIR) -> list[str]:
    """ledger 中 once_migration! 的 ID 顺序（不含 Rust 手写块）。"""
    ordered: list[str] = []
    seen: set[str] = set()
    for p in sorted((mig_dir / "ledger").glob("*.rs")):
        if p.name == "mod.rs":
            continue
        text = p.read_text()
        for m in ONCE_RE.finditer(text):
            i = m.group(1)
            if i in SKIP_IDS or i in seen:
                continue
            seen.add(i)
            ordered.append(i)
    return ordered


def collect_all_ids(mig_dir: Path = MIG_DIR) -> list[str]:
    ordered: list[str] = []
    seen: set[str] = set()
    for i in collect_ledger_ids(mig_dir):
        if i not in seen:
            seen.add(i)
            ordered.append(i)
    inc = mig_dir / "incremental.rs"
    if inc.exists():
        for i in ids_in_text(inc.read_text()):
            if i not in seen:
                seen.add(i)
                ordered.append(i)
    return ordered


def extract_once_blocks(text: str) -> list[tuple[str, int, str]]:
    blocks: list[tuple[str, int, str]] = []
    for m in ONCE_RE.finditer(text):
        start = m.start()
        id_ = m.group(1)
        if id_ in SKIP_IDS:
            continue
        i = text.find("(", start)
        depth = 0
        end = None
        for j in range(i, len(text)):
            c = text[j]
            if c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    k = j + 1
                    while k < len(text) and text[k] in " \t":
                        k += 1
                    if k < len(text) and text[k] == ";":
                        k += 1
                    end = k
                    break
        if end is None:
            print(f"# MANUAL: unclosed once_migration for {id_}", file=sys.stderr)
            continue
        blocks.append((id_, start, text[start:end].rstrip()))
    return blocks


def baseline_pos(text: str, baseline: str) -> int | None:
    for m in ONCE_RE.finditer(text):
        if m.group(1) == baseline:
            return m.start()
    return None


def cmd_legacy(legacy: Path, out: Path | None) -> int:
    ledger_ids = collect_ledger_ids()
    once_ids = collect_ledger_once_ids()
    if not ledger_ids or not once_ids:
        print("ERROR: no ledger migration ids found", file=sys.stderr)
        return 1
    baseline = once_ids[-1]
    ledger_set = set(ledger_ids)

    text = legacy.read_text()
    pos = baseline_pos(text, baseline)
    if pos is None:
        print(f"ERROR: baseline once_migration not found: {baseline}", file=sys.stderr)
        return 1

    new_blocks: list[tuple[str, str]] = []
    for id_, start, block in extract_once_blocks(text):
        if start <= pos:
            if id_ not in ledger_set and id_ != baseline:
                print(f"ERROR: id before baseline not in ledger: {id_}", file=sys.stderr)
                return 1
            continue
        if id_ in ledger_set:
            print(f"WARN: post-baseline id already in ledger, skip: {id_}", file=sys.stderr)
            continue
        new_blocks.append((id_, block))

    new_rust: list[str] = []
    for m in RUST_RE.finditer(text):
        id_ = m.group(1)
        if m.start() <= pos or id_ in ledger_set or id_ in SKIP_IDS:
            continue
        new_rust.append(id_)

    if not new_blocks and not new_rust:
        print("no new migrations to port", file=sys.stderr)
        return 0

    lines = [
        "// === ported from legacy migrations.rs — paste into incremental.rs ===",
        "use crate::db::migrations::once_migration;",
        "",
    ]
    for id_, block in new_blocks:
        lines.append(f"    // ported: {id_}")
        indented = "\n".join(
            ("    " + ln if ln.strip() else ln) for ln in block.splitlines()
        )
        lines.append(indented)
        if not indented.rstrip().endswith(";"):
            lines[-1] = indented + ";"
        lines.append("")

    if new_rust:
        lines.append("// # MANUAL: 以下 Rust 回填需手写移植：")
        for id_ in new_rust:
            lines.append(f'//   if !done.contains("{id_}") {{ ... }}')

    content = "\n".join(lines) + "\n"
    if out:
        out.write_text(content)
        print(f"wrote {out} ({len(new_blocks)} sql + {len(new_rust)} rust)", file=sys.stderr)
    else:
        sys.stdout.write(content)
    return 0


def cmd_check_duplicates() -> int:
    ids = collect_all_ids()
    seen: set[str] = set()
    dups: list[str] = []
    for i in ids:
        if i in seen:
            dups.append(i)
        else:
            seen.add(i)
    if dups:
        print("DUPLICATE ids:", ", ".join(dups))
        return 1
    ledger_n = len(collect_ledger_ids())
    print(f"ok: {ledger_n} ledger + {len(ids) - ledger_n} incremental ids")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--legacy", type=Path, help="旧单体 migrations.rs 路径")
    ap.add_argument("--out", type=Path, help="写出移植片段路径（默认 stdout）")
    ap.add_argument("--check-duplicates", action="store_true")
    args = ap.parse_args()

    if args.check_duplicates:
        return cmd_check_duplicates()
    if args.legacy:
        return cmd_legacy(args.legacy, args.out)
    ap.print_help()
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
