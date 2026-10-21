"""04_autoscale.py — auto-scale every channel (:AUT)."""

from omariglol import connect

ADDRESS = "192.168.100.36"  # <-- your instrument's address
PORT = 5555                 # Rigol's raw SCPI socket

with connect(ADDRESS, transport="raw", port=PORT) as scope:
    scope.write(":AUT")
    scope.query("*OPC?")  # blocks until auto-scale finishes
    print("auto-scale complete")
