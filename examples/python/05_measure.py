"""05_measure.py — read the standard measurements for CHAN1."""

from omariglol import connect

ADDRESS = "192.168.100.36"  # <-- your instrument's address
PORT = 5555                 # Rigol's raw SCPI socket

MEASUREMENTS = {
    "Vpp": ":MEAS:VPP?",
    "Vavg": ":MEAS:VAVG?",
    "Vrms": ":MEAS:VRMS?",
    "Freq": ":MEAS:FREQ?",
    "Period": ":MEAS:PER?",
}

with connect(ADDRESS, transport="raw", port=PORT) as scope:
    scope.write(":MEAS:SOUR CHAN1")
    for label, query in MEASUREMENTS.items():
        print(f"{label:>7}: {scope.query(query)}")
