"""03_single.py — arm a single acquisition and wait for it (:SING).

A single sweep runs once and then stops. *OPC? blocks until it completes.
"""

from omariglol import connect

ADDRESS = "192.168.100.36"  # <-- your instrument's address
PORT = 5555                 # Rigol's raw SCPI socket

with connect(ADDRESS, transport="raw", port=PORT) as scope:
    scope.write(":SING")
    scope.query("*OPC?")  # blocks until the sweep finishes
    print("single acquisition complete")
