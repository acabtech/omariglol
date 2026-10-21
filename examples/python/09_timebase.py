"""09_timebase.py — set the horizontal timebase and read it back."""

from omariglol import connect

ADDRESS = "192.168.100.36"  # <-- your instrument's address
PORT = 5555                 # Rigol's raw SCPI socket

with connect(ADDRESS, transport="raw", port=PORT) as scope:
    scope.write(":TIM:MODE MAIN")
    scope.write(":TIM:MAIN:SCAL 0.001")
    scope.write(":TIM:MAIN:OFFS 0")
    print("timebase scale:", scope.query(":TIM:MAIN:SCAL?"), "s/div")
