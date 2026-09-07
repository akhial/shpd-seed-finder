"""Run disjoint BETA-4 equipment streams against a frozen PR comparator.
Retains gzip-compressed oracle records, deviations, progress, and exact coverage.
"""
import argparse
import hashlib
import shutil
import concurrent.futures
import gzip
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("count", type=int)
parser.add_argument("workers", type=int, nargs="?", default=6)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--exe", type=Path, required=True)
parser.add_argument("--java", default=str(Path(os.environ.get("JAVA_21_HOME", os.environ.get("JAVA_HOME", ""))) / "bin/java") if os.environ.get("JAVA_21_HOME") or os.environ.get("JAVA_HOME") else "java")
args = parser.parse_args()
COUNT, WORKERS = args.count, args.workers
if not 0 < WORKERS <= COUNT:
    parser.error("require 0 < workers <= count")
WORK = args.output.resolve()
WORK.mkdir(parents=True, exist_ok=False)
JAVA = args.java
JAR = ROOT / "tooling/oracle-4.0/.work/ShatteredPD-v4.0.0-BETA-4-Java.jar"
jar_hash = hashlib.sha256(JAR.read_bytes()).hexdigest()
assert jar_hash == "76f6983e7b619267666621de9f1ecbbc3645d4925c2c446736987c3011b9dfd1", "wrong oracle JAR"
CP = os.pathsep.join(str(p) for p in [
    ROOT / "tooling/oracle-4.0/.work/batch-classes",
    ROOT / "tooling/java-finder/.work/classes", JAR,
])
EXE = WORK / ("equivalence.exe" if os.name == "nt" else "equivalence")
shutil.copy2(args.exe.resolve(), EXE)
manifest = {
    "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
    "dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()),
    "jar_sha256": jar_hash, "exe_sha256": hashlib.sha256(EXE.read_bytes()).hexdigest(),
    "start": 0, "count": COUNT, "workers": WORKERS, "floors": 24,
    "vault": True, "challenges": 0, "hero": "Warrior",
    "comparison_fields": ["floor", "source", "item", "upgrade", "cursed", "effect"],
    "comparison": "exact sorted multisets, duplicate entries retained",
    "scope": "catalog equipment only; quantity, cell, secret flag and accessibility groups are not compared",
}
(WORK / "manifest.json").write_text(json.dumps(manifest, indent=2))

def worker(index):
    start=COUNT*index//WORKERS
    end=COUNT*(index+1)//WORKERS
    prefix=WORK/f"shard-{index}"
    began=time.time()
    with open(str(prefix)+".java.log","wb") as je, open(str(prefix)+".rust.log","wb") as re, open(str(prefix)+".diff.jsonl","wb") as out, gzip.open(str(prefix)+".oracle.txt.gz","wb",compresslevel=1) as archive:
        rust=subprocess.Popen([str(EXE),str(start),str(end-start)],stdin=subprocess.PIPE,stdout=out,stderr=re)
        java=subprocess.Popen([JAVA,"-Xmx768m","-XX:+UseParallelGC","-XX:ActiveProcessorCount=1","-cp",CP,"com.shatteredpixel.shatteredpixeldungeon.BatchEquipmentOracle",str(start),str(end-start)],stdout=subprocess.PIPE,stderr=je)
        try:
            while True:
                block=java.stdout.read(65536)
                if not block:break
                archive.write(block)
                rust.stdin.write(block)
            java.stdout.close(); rust.stdin.close()
            jr=java.wait();rr=rust.wait()
        except BaseException:
            java.terminate();rust.terminate();java.wait();rust.wait();raise
    result={"shard":index,"start":start,"end_exclusive":end,"java_exit":jr,"rust_exit":rr,"elapsed":time.time()-began}
    Path(str(prefix)+".done.json").write_text(json.dumps(result,indent=2))
    return result

began=time.time()
(WORK/"run.json").write_text(json.dumps({"pid":os.getpid(),"started_at":began,"count":COUNT,"workers":WORKERS},indent=2))
with concurrent.futures.ThreadPoolExecutor(max_workers=WORKERS) as pool:
    jobs=[pool.submit(worker,n) for n in range(WORKERS)]
    while not all(j.done() for j in jobs):
        time.sleep(5)
        progress=[]
        for n in range(WORKERS):
            log=WORK/f"shard-{n}.rust.log"
            lines=log.read_text(errors="replace").splitlines() if log.exists() else []
            last=next((x for x in reversed(lines) if x.startswith("PROGRESS ")),"")
            progress.append({"shard":n,"status":last})
        (WORK/"progress.json").write_text(json.dumps({"elapsed":time.time()-began,"workers":progress},indent=2))
    results=[j.result() for j in jobs]
(WORK/"completed.json").write_text(json.dumps({"elapsed":time.time()-began,"shards":results},indent=2))
print(json.dumps(results),flush=True)
