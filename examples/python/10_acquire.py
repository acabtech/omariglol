"""10_acquire.py — configure acquisition type, averaging and memory depth."""

from omariglol import connect

ADDRESS = "192.168.100.36"  # <-- your instrument's address
PORT = 5555                 # Rigol's raw SCPI socket

with connect(ADDRESS, transport="raw", port=PORT) as scope:
    scope.write(":ACQ:TYPE AVER")
    scope.write(":ACQ:AVER 16")
    scope.write(":ACQ:MDEP AUTO")
    print("sample rate:", scope.query(":ACQ:SRAT?"), "Sa/s")
