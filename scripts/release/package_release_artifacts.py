#!/usr/bin/env python3
"""Package release artifacts and emit checksums with provenance metadata."""
from __future__ import annotations
import argparse, hashlib, json, platform, shutil, stat, tarfile, zipfile
from pathlib import Path

def sha256_file(path: Path) -> str:
    d=hashlib.sha256()
    with path.open('rb') as h:
        for c in iter(lambda:h.read(1024*1024), b''): d.update(c)
    return d.hexdigest()

def detect_target() -> str:
    m=platform.machine().lower(); s=platform.system().lower()
    arch='x86_64' if m in {'amd64','x86_64','x64'} else ('aarch64' if m in {'arm64','aarch64'} else (m or 'unknown'))
    return f"{arch}-unknown-linux-gnu" if s=='linux' else (f"{arch}-apple-darwin" if s=='darwin' else (f"{arch}-pc-windows-msvc" if s=='windows' else f"{arch}-{s}"))

def ensure_executable(path: Path) -> None:
    if platform.system().lower()!='windows': path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

def main() -> None:
    p=argparse.ArgumentParser()
    p.add_argument('--binary', required=True, type=Path); p.add_argument('--output-dir', required=True, type=Path)
    p.add_argument('--tag', required=True); p.add_argument('--bin-name', default='pulsedagd')
    p.add_argument('--repository', default=''); p.add_argument('--commit', default=''); p.add_argument('--run-id', default=''); p.add_argument('--run-attempt', default='')
    p.add_argument('--include-file', action='append', default=[], help='Extra repo file to include at archive root folder')
    a=p.parse_args(); b=a.binary.resolve(); out=a.output_dir.resolve(); out.mkdir(parents=True, exist_ok=True)
    if not b.exists(): raise SystemExit(f'Binary not found: {b}')
    target=detect_target(); base=f"{a.bin_name}-{a.tag}-{target}"; win=platform.system().lower()=='windows'; bname=f"{a.bin_name}.exe" if win else a.bin_name
    stage=out/base; shutil.rmtree(stage, ignore_errors=True); stage.mkdir(parents=True)
    sb=stage/bname; shutil.copy2(b,sb); ensure_executable(sb)
    included=[]
    for rel in a.include_file:
        src=Path(rel)
        if not src.exists() or not src.is_file(): raise SystemExit(f'Included file not found: {rel}')
        dst=stage/src.name; shutil.copy2(src,dst); included.append(src.name)
    arc=out/f"{base}.zip" if win else out/f"{base}.tar.gz"
    if win:
        with zipfile.ZipFile(arc,'w',compression=zipfile.ZIP_DEFLATED) as z:
            for f in sorted(stage.iterdir()): z.write(f, arcname=f"{base}/{f.name}")
    else:
        with tarfile.open(arc,'w:gz') as t:
            for f in sorted(stage.iterdir()): t.add(f, arcname=f"{base}/{f.name}")
    sha=sha256_file(arc); (out/f"{arc.name}.sha256").write_text(f"{sha}  {arc.name}\n",encoding='utf-8')
    (out/f"{arc.name}.json").write_text(json.dumps({"tag":a.tag,"archive":arc.name,"archive_sha256":sha,"archive_size_bytes":arc.stat().st_size,"target":target,"binary":bname,"included_files":included,"provenance":{"repository":a.repository,"commit":a.commit,"github_run_id":a.run_id,"github_run_attempt":a.run_attempt}},indent=2,sort_keys=True)+"\n",encoding='utf-8')
    shutil.rmtree(stage)
    print(f'Packaged: {arc}')

if __name__=='__main__': main()
