# GUI examples — Rigol DS1054Z

A set of small, focused Python scripts that drive a **Rigol DS1054Z** from the
desktop app's **Script** panel. Each script triggers **one** basic function, so
they double as a reference for the exact SCPI mnemonic behind each operation.

These are meant to be **copy-pasted straight into the GUI**: open the Script
panel, paste a script, and press **Run** (or `Ctrl+Enter`). `print()` output
streams into the panel's console, and `sample()` / `status()` calls feed the
live chart and status line.

## How they connect

The GUI's script engine runs in the same process as the app, but it does **not**
reuse the app's live connections — a script opens its own. Every example uses
the single-instrument API:

```python
from omariglol import connect

ADDRESS = "192.168.100.36"  # <-- set to the instrument you connected
PORT = 5555                 # Rigol's raw SCPI socket

with connect(ADDRESS, transport="raw", port=PORT) as scope:
    print(scope.idn())
```

Set `ADDRESS` to the address shown for your instrument in the **Workspace**
panel. The examples use Rigol's **raw SCPI socket on port 5555** (not the
LXI-standard 5025); for VXI-11 instead, drop the `transport`/`port` arguments:

```python
with connect(ADDRESS) as scope:
    ...
```

### USB (USBTMC)

Pass `transport="usb"` and a `usb:0xVVVV:0xPPPP[:SERIAL]` selector. The same
instrument object works over USB, so the rest of a script is unchanged:

```python
with connect("usb:0x1AB1:0x04CE", transport="usb") as scope:
    print(scope.idn())
```

See [`16_usb.py`](16_usb.py). On Linux, install
[`scripts/install-usb-udev.sh`](../../scripts/install-usb-udev.sh) once so libusb
can open the device.

### Multi-instrument scripts

For scripts that span several instruments, use the `Workspace` API instead. It
opens a saved workspace and addresses instruments by name, recording everything
on the shared timeline:

```python
from omariglol import Workspace

ws = Workspace.open("Default Workspace")
scope = ws.instruments["Rigol DS1054Z"]
print(scope.query("*IDN?"))
```

See [`15_workspace.py`](15_workspace.py) for a full example.

## The examples

| Script | Function | SCPI |
| --- | --- | --- |
| [`01_identify.py`](01_identify.py) | Identify the instrument | `*IDN?` |
| [`02_run_stop.py`](02_run_stop.py) | Start and stop acquisition | `:RUN`, `:STOP` |
| [`03_single.py`](03_single.py) | Arm a single sweep | `:SING` |
| [`04_autoscale.py`](04_autoscale.py) | Auto-scale all channels | `:AUT` |
| [`05_measure.py`](05_measure.py) | Read standard measurements | `:MEAS:*?` |
| [`06_trigger.py`](06_trigger.py) | Configure an edge trigger | `:TRIG:*` |
| [`07_math.py`](07_math.py) | Enable the math waveform | `:MATH:*` |
| [`08_channel.py`](08_channel.py) | Set up CHAN1 | `:CHAN1:*` |
| [`09_timebase.py`](09_timebase.py) | Set the horizontal timebase | `:TIM:*` |
| [`10_acquire.py`](10_acquire.py) | Configure acquisition | `:ACQ:*` |
| [`11_waveform.py`](11_waveform.py) | Read the waveform block | `:WAV:*` |
| [`12_screenshot.py`](12_screenshot.py) | Capture the screen to PNG | `:DISP:DATA?` |
| [`13_sweep_chart.py`](13_sweep_chart.py) | Live frequency chart | `:MEAS:FREQ?` |
| [`14_csv_log.py`](14_csv_log.py) | Log measurements to CSV | `:MEAS:*?` |
| [`15_workspace.py`](15_workspace.py) | Multi-instrument workspace | `Workspace` API |
| [`16_usb.py`](16_usb.py) | Identify the instrument over USB | `*IDN?`, `:MEAS:FREQ?` |

## See also

The CLI equivalents live in [`../scpi`](../scpi): the same operations as shell
scripts that invoke the `riglol` command-line tool.
