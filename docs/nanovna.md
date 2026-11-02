# NanoVNA-H over USBTMC

The `NanoVNA USBFix` project publishes an unofficial NanoVNA-H 1.2.50 image with
a composite CDC + USBTMC USB device. `omariglol-core` already has a generic
USBTMC transport, so the VNA needs no new transport code: it enumerates as a
USBTMC instrument and the normal `ScpiTransport` path applies.

## Selector

```text
usb:0x0483:0x5740
```

The VID/PID are the stock ST values. The interface class (`0xFE`, subclass
`0x03`) is the discriminator. `riglol usb list` shows the USBTMC status.

## Driver profile

`InstrumentDriver::from_idn` recognizes `NanoVNA...` and selects the **NanoVNA
USBFix** rich profile. It exposes:

| Group | Operations |
| --- | --- |
| `sweep` | `sweep_start`, `sweep_stop`, `sweep_points`, start/stop/points queries |
| `acquire` | `sweep_execute`, `pause`, `resume` |
| `output` | `frequency`, `frequency_query` |
| `measure` | `data_s11`, `data_s21`, `data_all` |
| `system` | `identify`, `reset`, `command`, `command_query` |

`command` and `command_query` map to the firmware's `:VNA:CMD` escape hatch, so
every stock NanoVNA shell command remains reachable.

## Example

```sh
examples/scpi/19_nanovna.sh
```

Or directly:

```sh
riglol scpi --usb usb:0x0483:0x5740 '*IDN?'
riglol scpi --usb usb:0x0483:0x5740 ':VNA:SWE:START 1000000'
riglol scpi --usb usb:0x0483:0x5740 ':VNA:SWE:STOP 30000000'
riglol scpi --usb usb:0x0483:0x5740 ':VNA:SWE:EXEC'
riglol scpi --usb usb:0x0483:0x5740 ':VNA:DATA:CHUNK? 0,0,32'
```

Full command reference: the
[NanoVNAUSBFix USB API](https://github.com/acabtech/NanoVNAUSBFix/blob/main/docs/usb-api.md).

## Notes

- The CDC ACM interface is still present. Opening `/dev/ttyACM*` talks to the
  stock shell, not to USBTMC.
- Screenshot (`capture`) over USBTMC is not supported because the firmware
  response buffer shares the LCD scratch buffer. Use CDC for screenshots.
