# Waveform capture

`riglol capture` reads a scope's acquisition memory and writes it to a file that
`numpy` can load directly. It exists because the DS1000Z-family raw memory path
is fiddly by hand and because an LLM agent should be able to analyse a capture
without streaming hundreds of thousands of samples through its context.

## Why a CLI, not an MCP wrapper

A capture is a **file-producing** operation, not a conversational one. The agent
runs:

```bash
riglol capture --raw --port 5555 192.168.1.50 /tmp/ch1.npy --source CHAN1
```

and then:

```python
import numpy as np
wave = np.load("/tmp/ch1.npy")     # shape (n, 2): time_s, voltage_v
print(wave.shape, wave[:, 1].max())
```

250 000 points never enter the model's context. The sidecar JSON carries the
scaling factors:

```bash
cat /tmp/ch1.json
```

If a follow-up question needs an actual sample, only that slice is read back.

## The DS1054Z RAW-mode constraints

`riglol capture` hides both quirks of `:WAV:MODE RAW`:

1. **The scope must be stopped.** `capture` sends `:STOP` first (disable with
   `--no-stop` only when the scope is already stopped).
2. **A single `:WAV:DATA?` returns at most 250 000 points.** `capture` reads the
   preamble, then walks the memory in 250 000-point windows using
   `:WAV:STAR`/`:WAV:STOP`, parsing the IEEE-488.2 block from each read and
   stitching the payloads into one contiguous sample vector.

The chunk size is configurable with `--chunk-points`. The stitched capture
reports how many chunks it used, so you can see the round-trip cost:

```
wrote /tmp/ch1.npy (12000000 points from 48 chunk(s), WORD format, raw mode)
```

## Modes

| `--mode` | Reads | Notes |
| --- | --- | --- |
| `raw` (default) | The full acquisition memory | Requires a stopped scope; chunked at 250 000 points |
| `norm` | Screen memory only | Use `--points` to cap the count |
| `max` | Peak-detect envelope over the display | Use `--points` to cap the count |

## Data formats

| `--data-format` | Bytes per point | Notes |
| --- | --- | --- |
| `word` (default) | 2 | Full vertical resolution; try `--word-order big` if values look pinned to the rails |
| `byte` | 1 | Half the transfer, coarser resolution |
| `ascii` | variable | Human-readable, slower; handy for a tiny capture |

The preamble (`format, type, points, count, xincrement, xorigin, xreference,
yincrement, yorigin, yreference`) is parsed and used to convert every raw code to
volts:

```
volts = (code - y_origin - y_reference) * y_increment
```

## Output files

| File | Contents |
| --- | --- |
| `<file>.npy` | NumPy v1.0 float64 array of shape `(n, 2)`: column 0 `time_s`, column 1 `voltage_v` |
| `<file>.csv` | CSV with the same two columns (write with `--csv`) |
| `<file>.json` | Metadata sidecar: source, mode, format, point count, chunk count, units and the preamble (disable with `--no-metadata`) |

The `.npy` extension is added automatically when the output path has none, so
`riglol capture ... /tmp/ch1` produces `/tmp/ch1.npy` and `/tmp/ch1.json`.

## Hardware notes (verified on a DS1054Z, firmware 00.04.04.SP4)

- **Memory depth follows the timebase.** `:ACQ:MDEP` is effectively read-only
  while the scope is on `AUTO` depth: setting `12K`/`1.2M`/`24M` is accepted with
  no error but the depth stays `AUTO`. To get a large RAW capture, slow the
  timebase (`:TIM:MAIN:SCAL`) instead — the depth grows to fill the window. At
  1 µs/div the RAW preamble reports 12 000 points; at 10 ms/div it reports
  6 000 000 (24 chunks).
- **Chunk stitching is exact.** Two captures of the same frozen frame, one at a
  250 000-point chunk size (24 chunks) and one at 100 000 (60 chunks), produced
  byte-identical `.npy` output. Chunk boundaries are continuous: `dt` stays at
  the preamble's `x_increment` across every boundary.
- **WORD byte order is little-endian.** A WORD capture and a BYTE capture of the
  same frame agreed to correlation 1.0 with identical min/max, so the default
  `--word-order little` is correct for this firmware.
- **Scaling validates against the scope's own measurements.** A captured 1 kHz
  square wave measured 0.052–2.99 V; the scope reported VPP 3.08 V. Recovering
  the period from the capture gave 1.000000 ms (999.999 Hz) by counting 120
  rising edges.
- Capture throughput over raw TCP (port 5555): 6 M points in ~48 s.

## Transports

USB (USBTMC) is the fastest path for a DS1054Z — typically 5–6× the throughput of
the same capture over Ethernet — so prefer it when the cable is attached:

```bash
riglol capture --usb usb:0x1AB1:0x04CE /tmp/ch1.npy --source CHAN1
```

Over the network, use Rigol's raw SCPI socket (port 5555, not the LXI-standard
5025):

```bash
riglol capture --raw --port 5555 192.168.1.50 /tmp/ch1.npy --source CHAN1
```

## Full option reference

```bash
riglol capture [OPTIONS] <ADDRESS> <FILE>

  --source <SOURCE>          CHAN1..CHAN4, MATH or a reference channel (default CHAN1)
  --mode <MODE>              raw | norm | max (default raw)
  --data-format <FORMAT>     word | byte | ascii (default word)
  --points <N>               point cap for norm/max
  --chunk-points <N>         max points per :WAV:DATA? (default 250000)
  --word-order <ORDER>       little | big (default little)
  --no-stop                  do not stop the scope first
  --csv                      write CSV instead of .npy
  --no-metadata              skip the .json sidecar
  --raw | --usb              transport
  --port <PORT>              port override
  --timeout <SECONDS>        socket timeout (default 10)
  --json                     print a JSON summary on stdout
```

## How it fits together

- [`crates/omariglol-core/src/waveform.rs`](../crates/omariglol-core/src/waveform.rs)
  owns the chunk stitching, preamble decoding and file writers.
- [`crates/omariglol-cli/src/main.rs`](../crates/omariglol-cli/src/main.rs) only
  resolves the transport and reports the result.
- The same core `capture` function is available to the desktop app and Python
  layer without re-implementing the chunk loop.
