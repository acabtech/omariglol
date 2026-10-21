"""11_waveform.py — read the waveform preamble and data block for CHAN1.

The data block is an IEEE-488.2 binary block, so query_raw() is used to get the
bytes rather than a decoded string.
"""

from omariglol import connect

ADDRESS = "192.168.100.36"  # <-- your instrument's address
PORT = 5555                 # Rigol's raw SCPI socket

with connect(ADDRESS, transport="raw", port=PORT) as scope:
    scope.write(":WAV:SOUR CHAN1")
    scope.write(":WAV:MODE NORM")
    scope.write(":WAV:FORM BYTE")
    scope.write(":WAV:POIN 1200")

    print("preamble:", scope.query(":WAV:PRE?"))

    data = scope.query_raw(":WAV:DATA?")
    print(f"received {len(data)} bytes")
    print("first 16 bytes:", data[:16].hex())
