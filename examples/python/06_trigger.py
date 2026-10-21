"""06_trigger.py — configure an edge trigger on CHAN1 and read its status."""

from omariglol import connect

ADDRESS = "192.168.100.36"  # <-- your instrument's address
PORT = 5555                 # Rigol's raw SCPI socket

with connect(ADDRESS, transport="raw", port=PORT) as scope:
    scope.write(":TRIG:MODE EDGE")
    scope.write(":TRIG:EDGE:SOUR CHAN1")
    scope.write(":TRIG:EDGE:SLOP POS")
    scope.write(":TRIG:EDGE:LEV 0.5")
    scope.write(":TRIG:SWE AUTO")
    print("trigger status:", scope.query(":TRIG:STAT?"))
