"""08_channel.py — set up CHAN1: display, coupling, probe, scale, offset."""

from omariglol import connect

ADDRESS = "192.168.100.36"  # <-- your instrument's address
PORT = 5555                 # Rigol's raw SCPI socket

with connect(ADDRESS, transport="raw", port=PORT) as scope:
    scope.write(":CHAN1:DISP ON")
    scope.write(":CHAN1:COUP DC")
    scope.write(":CHAN1:PROB 10")
    scope.write(":CHAN1:SCAL 0.5")
    scope.write(":CHAN1:OFFS 0")
    print("CHAN1 configured")
