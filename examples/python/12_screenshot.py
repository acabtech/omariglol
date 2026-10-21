"""12_screenshot.py — capture the instrument's screen to a PNG.

The GUI's Screenshot panel does this too; this shows the SCPI path. The DS1000Z
can emit PNG directly, which is far smaller than the default BMP.
"""

from pathlib import Path

from omariglol import connect, working_dir

ADDRESS = "192.168.100.36"  # <-- your instrument's address
PORT = 5555                 # Rigol's raw SCPI socket


def strip_block_header(block: bytes) -> bytes:
    """Strip an IEEE-488.2 definite-length block header (#<n><len>)."""
    if not block.startswith(b"#"):
        return block
    digits = block[1] - 0x30
    length = int(block[2 : 2 + digits])
    return block[2 + digits : 2 + digits + length]


with connect(ADDRESS, transport="raw", port=PORT) as scope:
    scope.write(":STOP")
    block = scope.query_raw(":DISP:DATA? ON,OFF,PNG")
    payload = strip_block_header(block)

    out = Path(working_dir() or ".") / "ds1054z-screen.png"
    out.write_bytes(payload)
    print(f"wrote {out} ({len(payload)} bytes)")
