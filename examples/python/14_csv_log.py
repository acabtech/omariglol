"""14_csv_log.py — log measurements to a CSV file.

CsvLogger writes an append-only CSV in the working directory the GUI set for
the run (falling back to the current directory).
"""

import time
from pathlib import Path

from omariglol import CsvLogger, checkpoint, connect, working_dir

ADDRESS = "192.168.100.36"  # <-- your instrument's address
PORT = 5555                 # Rigol's raw SCPI socket
READINGS = 20

out = Path(working_dir() or ".") / "ds1054z-measurements.csv"

with connect(ADDRESS, transport="raw", port=PORT) as scope, CsvLogger(
    str(out), ["t", "vpp", "vrms", "freq"]
) as log:
    scope.write(":MEAS:SOUR CHAN1")
    scope.write(":RUN")

    for i in range(READINGS):
        checkpoint()
        log.log(
            i,
            scope.query(":MEAS:VPP?"),
            scope.query(":MEAS:VRMS?"),
            scope.query(":MEAS:FREQ?"),
        )
        time.sleep(0.1)

print(f"wrote {out}")
