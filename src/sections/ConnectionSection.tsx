/**
 * Connection section — find an instrument, or connect directly
 * (UX journey §4.1, §6.1).
 *
 * Discovery controls, a results table with keyboard-selectable rows, and an
 * always-visible direct-connect form. Results stream in via `discover:device`
 * events so the table populates incrementally.
 */

import { useCallback, useEffect, useMemo, useState } from "react";

import {
  Checkbox,
  Field,
  NumberInput,
  Select,
  TextInput,
} from "@/components/Field";
import { Section } from "@/components/Section";
import { Alert, Badge, Button, EmptyState } from "@/components/ui";
import { useAsyncAction } from "@/hooks/useAsyncAction";
import * as api from "@/lib/api";
import { translateError } from "@/lib/errors";
import { loadLastInstrument } from "@/lib/persist";
import type { Device, Transport } from "@/lib/types";
import { useSession } from "@/state/session";

/** The transport a device will be opened with, preferring VXI-11. */
function transportFor(device: Device): Transport {
  return device.vxi11_port != null ? "vxi11" : "raw";
}

/** The port to pass for a device, or `undefined` to let the core decide. */
function portFor(device: Device, transport: Transport): number | undefined {
  if (transport === "raw") return device.raw_port ?? 5025;
  return undefined;
}

export default function ConnectionSection() {
  const {
    session,
    connecting,
    error: connectError,
    connect,
    clearError,
  } = useSession();

  const last = useMemo(() => loadLastInstrument(), []);

  const [mdns, setMdns] = useState(false);
  const [timeoutSecs, setTimeoutSecs] = useState(last?.timeoutSecs ?? 3);
  const [devices, setDevices] = useState<Device[]>([]);
  const [selectedAddress, setSelectedAddress] = useState<string | null>(null);
  const [discoverDone, setDiscoverDone] = useState(false);

  const [manualAddress, setManualAddress] = useState(last?.address ?? "");
  const [manualTransport, setManualTransport] = useState<Transport>(
    last?.transport ?? "vxi11",
  );
  const [manualPort, setManualPort] = useState<string>(
    last?.port != null ? String(last.port) : "",
  );

  const discovery = useAsyncAction(
    useCallback(
      (useMdns: boolean, timeout: number) => api.discover(useMdns, timeout),
      [],
    ),
  );

  // Stream results in as they are found.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void (async () => {
      const stop = await api.onDiscoverDevice((device) => {
        setDevices((prev) =>
          prev.some((d) => d.address === device.address)
            ? prev
            : [...prev, device],
        );
      });
      if (cancelled) stop();
      else unlisten = stop;
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const runDiscover = useCallback(async () => {
    clearError();
    setDevices([]);
    setSelectedAddress(null);
    setDiscoverDone(false);
    const result = await discovery.run(mdns, timeoutSecs);
    if (result) setDevices(result);
    setDiscoverDone(true);
  }, [clearError, discovery, mdns, timeoutSecs]);

  const selected = devices.find((d) => d.address === selectedAddress) ?? null;

  const onConnectSelected = useCallback(async () => {
    if (!selected) return;
    const transport = transportFor(selected);
    await connect({
      address: selected.address,
      transport,
      port: portFor(selected, transport),
      timeoutSecs,
    });
  }, [connect, selected, timeoutSecs]);

  const onConnectManual = useCallback(async () => {
    const port = manualPort.trim() === "" ? undefined : Number(manualPort);
    await connect({
      address: manualAddress.trim(),
      transport: manualTransport,
      port: Number.isFinite(port) ? port : undefined,
      timeoutSecs,
    });
  }, [connect, manualAddress, manualPort, manualTransport, timeoutSecs]);

  const onRowKeyDown = (
    event: React.KeyboardEvent<HTMLTableRowElement>,
    device: Device,
  ) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      setSelectedAddress(device.address);
    }
  };

  const connectErrorText = connectError ? translateError(connectError) : null;
  const discoverErrorText = discovery.error
    ? translateError(discovery.error)
    : null;

  return (
    <Section
      id="connection"
      title="Connection"
      description="Find an instrument on the network, or connect directly."
      actions={
        <Button variant="primary" busy={discovery.busy} onClick={runDiscover}>
          Discover
        </Button>
      }
    >
      <div className="toolbar">
        <Checkbox
          id="search-mdns"
          label="mDNS"
          checked={mdns}
          onChange={(event) => setMdns(event.target.checked)}
        />
        <Field label="Timeout" htmlFor="search-timeout" className="field--inline">
          <NumberInput
            id="search-timeout"
            min={1}
            max={60}
            value={timeoutSecs}
            onChange={(event) =>
              setTimeoutSecs(Math.max(1, Number(event.target.value) || 1))
            }
          />
        </Field>
        <span className="toolbar__unit">seconds</span>
        {discovery.busy && (
          <span className="progress-line" aria-live="polite">
            listening…
          </span>
        )}
      </div>

      {connectErrorText && (
        <Alert tone="error" title="Could not connect" onDismiss={clearError}>
          {connectErrorText.message}
          {connectErrorText.recovery && (
            <span className="alert__recovery"> {connectErrorText.recovery}</span>
          )}
        </Alert>
      )}

      {discoverErrorText && (
        <Alert tone="error" title="Discovery failed">
          {discoverErrorText.message}
        </Alert>
      )}

      {devices.length > 0 ? (
        <>
          <div className="table-wrap">
            <table className="table table--selectable">
              <thead>
                <tr>
                  <th scope="col">Address</th>
                  <th scope="col">Name</th>
                  <th scope="col">VXI-11</th>
                  <th scope="col">Raw</th>
                  <th scope="col">Method</th>
                </tr>
              </thead>
              <tbody>
                {devices.map((device) => {
                  const isSelected = device.address === selectedAddress;
                  return (
                    <tr
                      key={device.address}
                      tabIndex={0}
                      aria-selected={isSelected}
                      className={isSelected ? "is-selected" : undefined}
                      onClick={() => setSelectedAddress(device.address)}
                      onKeyDown={(event) => onRowKeyDown(event, device)}
                    >
                      <td className="mono">{device.address}</td>
                      <td>{device.mdns_name ?? device.hostname ?? "—"}</td>
                      <td className="mono">{device.vxi11_port ?? "—"}</td>
                      <td className="mono">{device.raw_port ?? "—"}</td>
                      <td>
                        <Badge>{device.method}</Badge>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          <div className="action-row">
            <Button
              variant="primary"
              disabled={!selected || connecting}
              busy={connecting}
              onClick={onConnectSelected}
            >
              Connect
            </Button>
            {selected && (
              <span className="action-row__hint">
                via {transportFor(selected)}
                {portFor(selected, transportFor(selected)) != null
                  ? `:${portFor(selected, transportFor(selected))}`
                  : ""}
              </span>
            )}
          </div>
        </>
      ) : discoverDone ? (
        <>
          <EmptyState title="No instruments found">
            Check the instrument is powered on and on this subnet, or connect
            directly below.
          </EmptyState>
          <Alert tone="warning">
            No response within {timeoutSecs} s. Try a longer timeout or enable
            mDNS.
          </Alert>
        </>
      ) : (
        <p className="hint-line">
          Run a discovery to find instruments on this subnet, or connect
          directly below.
        </p>
      )}

      <div className="divider" />

      <div className="subgroup">
        <h3 className="subgroup__title">Connect directly</h3>
        <div className="form-grid">
          <Field label="Address" htmlFor="manual-address">
            <TextInput
              id="manual-address"
              placeholder="192.168.1.50"
              value={manualAddress}
              onChange={(event) => setManualAddress(event.target.value)}
            />
          </Field>
          <Field label="Transport" htmlFor="manual-transport">
            <Select
              id="manual-transport"
              value={manualTransport}
              onChange={(event) =>
                setManualTransport(event.target.value as Transport)
              }
            >
              <option value="vxi11">vxi11</option>
              <option value="raw">raw</option>
            </Select>
          </Field>
          <Field
            label="Port"
            htmlFor="manual-port"
            help="Blank uses the default."
          >
            <TextInput
              id="manual-port"
              inputMode="numeric"
              placeholder="5025"
              value={manualPort}
              onChange={(event) => setManualPort(event.target.value)}
            />
          </Field>
        </div>
        <div className="action-row">
          <Button
            variant="primary"
            busy={connecting}
            disabled={manualAddress.trim() === ""}
            onClick={onConnectManual}
          >
            Connect
          </Button>
          {session && (
            <span className="action-row__hint action-row__hint--ok">
              connected: {session.idn}
            </span>
          )}
        </div>
      </div>
    </Section>
  );
}
