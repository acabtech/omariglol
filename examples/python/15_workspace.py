"""15_workspace.py — drive a workspace and its instruments by name.

Unlike the other examples, this uses the multi-instrument `Workspace` API. It
creates a workspace, binds an instrument at the default address, and records the
run on the shared timeline.
"""

from omariglol import Workspace

ADDRESS = "192.168.100.36"  # <-- your instrument's address
PORT = 5555                 # Rigol's raw SCPI socket

ws = Workspace.create("DS1054Z Demo")
scope = ws.bind("scope", ADDRESS, transport="raw", port=PORT)

print(scope.query("*IDN?"))

with ws.run("frequency-check") as run:
    scope.write(":MEAS:SOUR CHAN1")
    scope.write(":RUN")
    run.wait(scope, "*OPC?")
    freq = float(scope.query(":MEAS:FREQ?"))
    run.record(frequency=freq)
    print(f"frequency: {freq:.3f} Hz")
