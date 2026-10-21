"""13_sweep_chart.py — stream frequency measurements to the live chart.

Emits one sample() per reading, so the GUI's chart updates while the script
runs, and status() updates the progress line above the console.
"""

import time

from omariglol import checkpoint, connect, sample, status

ADDRESS = "192.168.100.36"  # <-- your instrument's address
PORT = 5555                 # Rigol's raw SCPI socket
READINGS = 50

with connect(ADDRESS, transport="raw", port=PORT) as scope:
    scope.write(":MEAS:SOUR CHAN1")
    scope.write(":RUN")

    for i in range(READINGS):
        checkpoint()
        freq = float(scope.query(":MEAS:FREQ?"))
        sample("frequency", freq)
        status(f"reading {i + 1}/{READINGS}: {freq:.3f} Hz")
        time.sleep(0.1)

    print(f"captured {READINGS} readings")
