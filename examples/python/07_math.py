"""07_math.py — enable the math waveform (CHAN1 + CHAN2)."""

from omariglol import connect

ADDRESS = "192.168.100.36"  # <-- your instrument's address
PORT = 5555                 # Rigol's raw SCPI socket

with connect(ADDRESS, transport="raw", port=PORT) as scope:
    scope.write(":MATH:OPER ADD")
    scope.write(":MATH:SOUR1 CHAN1")
    scope.write(":MATH:SOUR2 CHAN2")
    scope.write(":MATH:DISP ON")
    print("math waveform enabled (CHAN1 + CHAN2)")
