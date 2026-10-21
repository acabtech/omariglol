"""01_identify.py — identify the instrument with *IDN?.

Paste into the GUI Script panel and press Run (Ctrl+Enter).
Set ADDRESS to the instrument you connected in the Workspace panel.
"""

from omariglol import connect

ADDRESS = "192.168.100.36"  # <-- your instrument's address
PORT = 5555                 # Rigol's raw SCPI socket

with connect(ADDRESS, transport="raw", port=PORT) as scope:
    print(scope.idn())
