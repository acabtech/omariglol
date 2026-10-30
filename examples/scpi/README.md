# CLI examples — Rigol DS1054Z

A set of small, focused shell scripts that drive a **Rigol DS1054Z** through the
`riglol` command-line interface. Each script triggers **one** basic function, so
they double as a reference for the exact SCPI mnemonic behind each operation.

These are *not* standalone SCPI clients: every script shells out to `riglol`
(mostly `riglol scpi`), which owns the transport, framing and timeouts. That is
deliberate — the examples show how to *use the tool*, not how to reimplement it.

## Prerequisites

Build the CLI; `make` publishes it to `bin/riglol`, which the examples invoke
directly. The path is resolved relative to each script, so the examples work
from any directory:

```sh
make                 # builds bin/riglol
```

Set `RIGLOL=/path/to/riglol` to use a different binary.

## Running an example

Every script takes the instrument address as its first argument, falling back to
`$RIGLOL_ADDRESS` and then `192.168.100.36`:

```sh
./02_run.sh 192.168.100.36
RIGLOL_ADDRESS=192.168.100.36 ./06_measure.sh
```

By default the examples use Rigol's **raw SCPI socket on port 5555** (not the
LXI-standard 5025). To use VXI-11 instead:

```sh
RIGLOL_TRANSPORT=vxi11 ./01_identify.sh 192.168.100.36
```

Slow operations (auto-scale, `*RST`, `*TST?`) use a 10 s socket timeout; raise it
with `RIGLOL_TIMEOUT`:

```sh
RIGLOL_TIMEOUT=30 ./17_reset.sh
```

## USB (USBTMC)

[`18_usb.sh`](18_usb.sh) drives the same instrument over USB. Pass a
`usb:0xVVVV:0xPPPP[:SERIAL]` selector, or use the default DS1054Z ids. On Linux,
install the bundled udev rule once:

```sh
sudo scripts/install-usb-udev.sh
./18_usb.sh
./18_usb.sh usb:0x1AB1:0x04CE:DS1ZA203012565   # pin to a serial
```

`riglol usb list` shows every attached device and whether it exposes a USBTMC
interface.

## The examples

[`00_smoke_test.sh`](00_smoke_test.sh) runs every function below end to end,
printing PASS/FAIL per step and a summary. It takes an optional delay between
steps (default 1 s):

```sh
./00_smoke_test.sh 192.168.100.36 0.5   # 0.5 s between steps
RIGLOL_DELAY=2 ./00_smoke_test.sh       # or via the environment
```

| Script | Function | SCPI |
| --- | --- | --- |
| [`00_smoke_test.sh`](00_smoke_test.sh) | Run every function end to end | all of the below |
| [`01_identify.sh`](01_identify.sh) | Identify the instrument | `*IDN?` |
| [`02_run.sh`](02_run.sh) | Start acquisition | `:RUN` |
| [`03_stop.sh`](03_stop.sh) | Stop acquisition | `:STOP` |
| [`04_single.sh`](04_single.sh) | Arm a single sweep | `:SING` |
| [`05_autoscale.sh`](05_autoscale.sh) | Auto-scale all channels | `:AUT` |
| [`06_measure.sh`](06_measure.sh) | Read standard measurements | `:MEAS:*?` |
| [`07_trigger.sh`](07_trigger.sh) | Configure an edge trigger | `:TRIG:*` |
| [`08_math.sh`](08_math.sh) | Enable the math waveform | `:MATH:*` |
| [`09_channel.sh`](09_channel.sh) | Set up CHAN1 | `:CHAN1:*` |
| [`10_timebase.sh`](10_timebase.sh) | Set the horizontal timebase | `:TIM:*` |
| [`11_acquire.sh`](11_acquire.sh) | Configure acquisition | `:ACQ:*` |
| [`12_waveform.sh`](12_waveform.sh) | Read the waveform block | `:WAV:*` |
| [`13_screenshot.sh`](13_screenshot.sh) | Capture the screen to PNG | `:DISP:DATA?` |
| [`14_benchmark.sh`](14_benchmark.sh) | Measure round-trip performance | `riglol benchmark` |
| [`15_discover.sh`](15_discover.sh) | Find instruments on the network | `riglol discover` |
| [`16_driver.sh`](16_driver.sh) | Show the named command surface | `riglol driver` |
| [`17_reset.sh`](17_reset.sh) | Reset and self-test | `*CLS`, `*RST`, `*TST?` |
| [`18_usb.sh`](18_usb.sh) | Enumerate and identify over USB | `*IDN?`, `:MEAS:FREQ?` |

## Interactive use

For ad-hoc commands, `riglol scpi` with no command starts a REPL:

```sh
riglol scpi 192.168.100.36 --raw --port 5555
> *IDN?
> :MEAS:FREQ?
> quit
```

## See also

The GUI equivalents live in [`../python`](../python): the same operations as
Python scripts you paste into the desktop app's Script panel.
