/**
 * Discovery panel (discovery subsystem).
 *
 * Surfaces the multi-strategy discovery engine: the detected network
 * configuration, the strategy controls, and the cached results. Each cached
 * instrument can be added to the active workspace, and a manual address can be
 * added directly.
 */

import { useCallback, useState } from "react";

import { Checkbox, Field, NumberInput, Select, TextInput } from "@/components/Field";
import { Section } from "@/components/Section";
import { Alert, Badge, Button, EmptyState } from "@/components/ui";
import { translateError } from "@/lib/errors";
import type { Transport } from "@/lib/types";
import { useWorkspace } from "@/state/workspace";

export default function DiscoverySection() {
  const {
    cached,
    network,
    scanning,
    scan,
    invalidateCache,
    addInstrument,
    error,
    clearError,
  } = useWorkspace();

  const [useBroadcast, setUseBroadcast] = useState(true);
  const [useMdns, setUseMdns] = useState(true);
  const [scanSubnet, setScanSubnet] = useState(true);
  const [incremental, setIncremental] = useState(true);
  const [timeoutSecs, setTimeoutSecs] = useState(3);
  const [ttlSecs, setTtlSecs] = useState(300);
  const [extraHosts, setExtraHosts] = useState("");

  const [manualAddress, setManualAddress] = useState("");
  const [manualTransport, setManualTransport] = useState<Transport>("vxi11");
  const [manualPort, setManualPort] = useState("");

  const runScan = useCallback(() => {
    void scan({
      useBroadcast,
      useMdns,
      scanSubnet,
      incremental,
      timeoutSecs,
      ttlSecs,
      extraHosts: extraHosts
        .split(/[\s,]+/)
        .map((host) => host.trim())
        .filter(Boolean),
    });
  }, [
    scan,
    useBroadcast,
    useMdns,
    scanSubnet,
    incremental,
    timeoutSecs,
    ttlSecs,
    extraHosts,
  ]);

  const addCached = useCallback(
    (address: string, transport: Transport, port?: number) => {
      void addInstrument({ address, transport, port, timeoutSecs });
    },
    [addInstrument, timeoutSecs],
  );

  const addManual = useCallback(() => {
    const address = manualAddress.trim();
    if (!address) return;
    const port = manualPort.trim() === "" ? undefined : Number(manualPort);
    void addInstrument({
      address,
      transport: manualTransport,
      port: Number.isFinite(port) ? port : undefined,
      timeoutSecs,
    });
    setManualAddress("");
    setManualPort("");
  }, [addInstrument, manualAddress, manualPort, manualTransport, timeoutSecs]);

  const errorText = error ? translateError(error) : null;

  return (
    <Section
      id="discovery"
      title="Discovery"
      description="Find instruments across the network, with a cached, incremental scan."
      actions={
        <Button variant="primary" busy={scanning} onClick={runScan}>
          Scan
        </Button>
      }
    >
      {errorText && (
        <Alert tone="error" title="Discovery error" onDismiss={clearError}>
          {errorText.message}
        </Alert>
      )}

      <div className="network-card">
        <h3 className="subgroup__title">Network</h3>
        {network?.primary ? (
          <dl className="network-card__meta">
            <div className="network-card__row">
              <dt>Interface</dt>
              <dd className="mono">
                {network.primary.name}
                {network.primary.vlan != null
                  ? ` (vlan ${network.primary.vlan})`
                  : ""}
              </dd>
            </div>
            <div className="network-card__row">
              <dt>Subnet</dt>
              <dd className="mono">
                {network.primary.address}/{network.primary.prefix_len}
              </dd>
            </div>
            <div className="network-card__row">
              <dt>Gateway</dt>
              <dd className="mono">{network.gateway ?? "—"}</dd>
            </div>
            <div className="network-card__row">
              <dt>Link-local</dt>
              <dd>{network.primary.is_link_local ? "yes" : "no"}</dd>
            </div>
          </dl>
        ) : (
          <p className="hint-line">No routable interface detected.</p>
        )}
      </div>

      <div className="toolbar">
        <Checkbox
          id="discovery-broadcast"
          label="VXI-11 broadcast"
          checked={useBroadcast}
          onChange={(event) => setUseBroadcast(event.target.checked)}
        />
        <Checkbox
          id="discovery-mdns"
          label="mDNS"
          checked={useMdns}
          onChange={(event) => setUseMdns(event.target.checked)}
        />
        <Checkbox
          id="discovery-subnet"
          label="Subnet scan"
          checked={scanSubnet}
          onChange={(event) => setScanSubnet(event.target.checked)}
        />
        <Checkbox
          id="discovery-incremental"
          label="Incremental"
          checked={incremental}
          onChange={(event) => setIncremental(event.target.checked)}
        />
        <Field label="Listen" htmlFor="discovery-timeout" className="field--inline">
          <NumberInput
            id="discovery-timeout"
            min={1}
            max={60}
            value={timeoutSecs}
            onChange={(event) =>
              setTimeoutSecs(Math.max(1, Number(event.target.value) || 1))
            }
          />
        </Field>
        <span className="toolbar__unit">s</span>
        <Field label="TTL" htmlFor="discovery-ttl" className="field--inline">
          <NumberInput
            id="discovery-ttl"
            min={1}
            max={86400}
            value={ttlSecs}
            onChange={(event) =>
              setTtlSecs(Math.max(1, Number(event.target.value) || 1))
            }
          />
        </Field>
        <span className="toolbar__unit">s</span>
      </div>

      <Field
        label="Extra hosts"
        htmlFor="discovery-hosts"
        help="Static addresses or other subnets, space- or comma-separated."
      >
        <TextInput
          id="discovery-hosts"
          className="mono"
          placeholder="192.168.1.50 10.0.0.7"
          value={extraHosts}
          onChange={(event) => setExtraHosts(event.target.value)}
        />
      </Field>

      {scanning && (
        <p className="progress-line" aria-live="polite">
          scanning…
        </p>
      )}

      <div className="divider" />

      <div className="subgroup">
        <div className="subgroup__head">
          <h3 className="subgroup__title">Cached instruments</h3>
          <Button onClick={() => void invalidateCache()} disabled={cached.length === 0}>
            Clear cache
          </Button>
        </div>
        {cached.length === 0 ? (
          <EmptyState title="No cached instruments">
            Run a scan to populate the cache, or add an address manually below.
          </EmptyState>
        ) : (
          <div className="table-wrap">
            <table className="table">
              <thead>
                <tr>
                  <th scope="col">Address</th>
                  <th scope="col">Model</th>
                  <th scope="col">Serial</th>
                  <th scope="col">VLAN</th>
                  <th scope="col">Method</th>
                  <th scope="col">Actions</th>
                </tr>
              </thead>
              <tbody>
                {cached.map((entry) => {
                  const transport: Transport =
                    entry.device.vxi11_port != null ? "vxi11" : "raw";
                  const port =
                    transport === "vxi11"
                      ? entry.device.vxi11_port
                      : entry.device.raw_port;
                  return (
                    <tr key={entry.device.address}>
                      <td className="mono">{entry.device.address}</td>
                      <td>{entry.model || "—"}</td>
                      <td className="mono">{entry.serial || "—"}</td>
                      <td className="mono">{entry.vlan ?? "—"}</td>
                      <td>
                        <Badge>{entry.device.method}</Badge>
                      </td>
                      <td>
                        <Button
                          onClick={() =>
                            addCached(entry.device.address, transport, port)
                          }
                        >
                          Add
                        </Button>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      <div className="divider" />

      <div className="subgroup">
        <h3 className="subgroup__title">Add manually</h3>
        <div className="form-grid">
          <Field label="Address" htmlFor="discovery-manual-address">
            <TextInput
              id="discovery-manual-address"
              placeholder="192.168.1.50"
              value={manualAddress}
              onChange={(event) => setManualAddress(event.target.value)}
            />
          </Field>
          <Field label="Transport" htmlFor="discovery-manual-transport">
            <Select
              id="discovery-manual-transport"
              value={manualTransport}
              onChange={(event) =>
                setManualTransport(event.target.value as Transport)
              }
            >
              <option value="vxi11">vxi11</option>
              <option value="raw">raw</option>
              <option value="usb">usb</option>
            </Select>
          </Field>
          <Field label="Port" htmlFor="discovery-manual-port" help="Blank uses the default.">
            <TextInput
              id="discovery-manual-port"
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
            disabled={manualAddress.trim() === ""}
            onClick={addManual}
          >
            Add to workspace
          </Button>
        </div>
      </div>
    </Section>
  );
}
