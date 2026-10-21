"""02_run_stop.py — start and stop acquisition (:RUN / :STOP).

Runs the scope for a moment, then stops it. The wait loop calls checkpoint()
so the GUI's Cancel button can interrupt the script.
"""

import time

from omariglol import checkpoint, connect

ADDRESS = "192.168.100.36"  # <-- your instrument's address
PORT = 5555                 # Rigol's raw SCPI socket

with connect(ADDRESS, transport="raw", port=PORT) as scope:
    print(scope.idn())

    scope.write(":RUN")
    print("acquisition started")

    # Let it run for ~1 s, staying interruptible.
    for _ in range(10):
        checkpoint()
        time.sleep(0.1)

    scope.write(":STOP")
    print("acquisition stopped")
