"""Exercise the comparator's failure paths using one retained RC1 oracle record.

python tooling/parity/test_equivalence.py --exe PATH --oracle-archive SHARD.oracle.txt.gz
"""
import argparse
import gzip
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--exe", type=Path, required=True)
parser.add_argument("--oracle-archive", type=Path, required=True)
args = parser.parse_args()
with gzip.open(args.oracle_archive, "rt", encoding="utf-8") as archive:
    record = next(line.strip() for line in archive if not line.startswith("BENCH "))
fields = record.split("|")
assert len(fields) == 4, "requires an RC1 item-and-cell stream"
seed = int(fields[0])


def check(label, value, success=False, count=1):
    result = subprocess.run(
        [str(args.exe.resolve()), str(seed), str(count)],
        input=value, text=True, capture_output=True,
    )
    assert (result.returncode == 0) == success, (label, result.stdout, result.stderr)
    print(f"PASS {label}")


check("exact record", record + "\n", success=True)
changed = fields.copy()
entries = changed[1].split(";")
item = entries[0].split(",")
item[3] = str(int(item[3]) + 1)
entries[0] = ",".join(item)
changed[1] = ";".join(entries)
check("changed item upgrade", "|".join(changed) + "\n")
changed = fields.copy()
changed[1] += ";" + changed[1].split(";")[0]
check("extra duplicate item", "|".join(changed) + "\n")
changed = fields.copy()
header, rest = changed[2].split(":", 1)
tile, rest = rest.split(",", 1)
changed[2] = f"{header}:{int(tile) ^ 1},{rest}"
check("changed terrain cell", "|".join(changed) + "\n")
changed = fields.copy()
locations = changed[3].split(";")
location = locations[0].split(",")
location[2] = str(int(location[2]) + 1)
locations[0] = ",".join(location)
changed[3] = ";".join(locations)
check("moved equipment cell", "|".join(changed) + "\n")
changed = fields.copy()
changed[2] = ";".join(changed[2].split(";")[:-1])
check("missing floor", "|".join(changed) + "\n")
check("missing seed", "")
check("duplicate seed", (record + "\n") * 2, count=2)
check("oracle generation error", f"ERROR|{seed}|negative control\n")
