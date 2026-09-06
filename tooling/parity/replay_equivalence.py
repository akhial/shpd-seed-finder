"""Replay retained oracle streams, following an active run without regenerating Java floors."""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time
import zlib

work = Path(sys.argv[1]).resolve()
exe = Path(sys.argv[2]).resolve()
manifest = json.loads((work / "manifest.json").read_text())
count, workers = manifest["count"], manifest["workers"]
if (work / "fixed-progress.json").exists():
    raise SystemExit("replay already exists; use a fresh archive directory to preserve evidence")
(work / "fixed-manifest.json").write_text(json.dumps({
    "exe": str(exe), "exe_sha256": hashlib.sha256(exe.read_bytes()).hexdigest(),
    "oracle_manifest": manifest,
}, indent=2))

def replay(index):
    start, end = count * index // workers, count * (index + 1) // workers
    prefix = work / f"shard-{index}"
    decoder = zlib.decompressobj(16 + zlib.MAX_WBITS)
    with open(str(prefix) + ".fixed.diff.jsonl", "wb") as output, open(str(prefix) + ".fixed.log", "wb") as log:
        process = subprocess.Popen([str(exe), str(start), str(end - start)], stdin=subprocess.PIPE, stdout=output, stderr=log)
        try:
            with open(str(prefix) + ".oracle.txt.gz", "rb") as archive:
                while True:
                    block = archive.read(65536)
                    if block:
                        process.stdin.write(decoder.decompress(block))
                    elif Path(str(prefix) + ".done.json").exists():
                        # The writer closes the archive before publishing its done marker.
                        block = archive.read(65536)
                        if block:
                            process.stdin.write(decoder.decompress(block))
                            continue
                        break
                    else:
                        process.stdin.flush()
                        time.sleep(2)
            assert decoder.eof, "truncated gzip oracle stream"
            process.stdin.write(decoder.flush())
            process.stdin.close()
            rc = process.wait()
        except BaseException:
            process.terminate()
            process.wait()
            raise
    return {"shard": index, "start": start, "end_exclusive": end, "exit": rc}

began = time.time()
with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
    jobs = [pool.submit(replay, i) for i in range(workers)]
    while not all(job.done() for job in jobs):
        time.sleep(5)
        progress = []
        for i in range(workers):
            log = work / f"shard-{i}.fixed.log"
            lines = log.read_text(errors="replace").splitlines() if log.exists() else []
            progress.append({"shard": i, "status": next((line for line in reversed(lines) if line.startswith("PROGRESS ")), "")})
        (work / "fixed-progress.json").write_text(json.dumps({"elapsed": time.time() - began, "workers": progress}, indent=2))
    result = [job.result() for job in jobs]
(work / "fixed-completed.json").write_text(json.dumps(result, indent=2))
print(json.dumps(result))
