# M0 spike — portmapper-only LXI detection

**Status:** complete
**Milestone:** M0 (lxi-tools parity foundation)
**Related code:** [`transport/rpc.rs`](../../crates/omariglol-core/src/transport/rpc.rs), [`discovery.rs`](../../crates/omariglol-core/src/discovery.rs)
**Runnable experiment:** [`examples/portmapper_spike.rs`](../../crates/omariglol-core/examples/portmapper_spike.rs)

## Question

Can discovery identify an LXI instrument by talking **only** to its portmapper
(`RPCBIND`), so that repeated discovery never creates and destroys VXI-11
links?

During the USB/LXI parity work the Rigol DS1054Z became unstable after
discovery opened a VXI-11 link per probe. Opening that many links appeared to
wedge the scope's LAN stack. A detection path that never calls `create_link`,
`device_write`, `device_read` or `destroy_link` should avoid that failure mode.

## Hypothesis

Repeated `PMAP_GETPORT` / `PMAP_DUMP` calls are non-disruptive and stable, and
a `GETPORT` for the VXI-11 `DEVICE_CORE` program is enough to identify an LXI
instrument.

A secondary expectation was that `PMAP_DUMP` and `PMAP_GETPORT` produce the
same program table.

## Method

The spike program issues only:

* TCP `PMAP_GETPORT` for `DEVICE_CORE`
* UDP `PMAP_GETPORT` for `DEVICE_CORE`
* TCP `PMAP_DUMP` every tenth iteration

It never creates a VXI-11 link.

```sh
cargo run -p omariglol-core --example portmapper_spike -- \
    192.168.100.36 30 100
```

After the run, a normal VXI-11 `*IDN?` query checks that the instrument is still
healthy.

## Results

Against the Rigol DS1054Z at `192.168.100.36` (30 iterations, 100 ms apart,
750 ms per-request deadline):

| Probe | Success | Latency |
| --- | --- | --- |
| TCP `GETPORT` (VXI-11 `DEVICE_CORE`) | 30/30 | p50 226 ms, p95 339 ms |
| UDP `GETPORT` (VXI-11 `DEVICE_CORE`) | 30/30 | p50 0.6 ms, p95 0.9 ms |
| TCP `PMAP_DUMP` | 0/3 initially | malformed-XDR error |

`DEVICE_CORE` was consistently reported on TCP port 618. A VXI-11 `*IDN?` after
the run succeeded, so the instrument was not disturbed.

### Finding 1 — `PMAP_DUMP` parsing was wrong

The first run failed every `PMAP_DUMP` with:

```
XDR: need 4 bytes at offset 104 but buffer has 104
```

XDR encodes a linked list as a boolean **presence flag before each element**:

```
TRUE, map, TRUE, map, …, FALSE
```

[`parse_pmaplist`](../../crates/omariglol-core/src/transport/rpc.rs) instead
read the boolean **after** each map, which misaligned every field. With five
mappings the wire payload is `5 × (4 + 16) + 4 = 104` bytes, exactly the offset
the error reported.

This also explains why broadcast `PMAP_DUMP` discovery never found anything on
this network: the reply was discarded as malformed. The parser and its tests
were corrected to the presence-flag form; `PMAP_DUMP` now succeeds.

### Finding 2 — UDP `GETPORT` is far cheaper than TCP

A UDP `GETPORT` is one datagram and answered in under a millisecond. A TCP
`GETPORT` needs a connection, a full RPC round trip, and teardown, and measured
~226 ms p50 on the same instrument. There is no benefit to TCP here because
`RPCBIND` is specified to answer on both transports.

## Decision

1. `discover` detects LXI instruments through a **portmapper-only** probe and
   never opens a VXI-11 link during discovery.
2. The probe uses **UDP `GETPORT`**, with the scan's short per-host deadline
   (300 ms) and no TCP fallback, so silent hosts do not pay two deadlines.
3. `PMAP_DUMP` uses the spec-correct presence-flag decoding, which also repairs
   broadcast discovery.

Measured effect on a `/24` scan of the lab network:

| Discovery path | Wall clock |
| --- | --- |
| TCP `GETPORT` fallback | ~3.46 s |
| UDP `GETPORT` + TCP fallback | ~5.75 s |
| UDP `GETPORT` only (adopted) | **~2.11 s** |

## Consequences and follow-ups

* Instruments identified this way show an empty model/serial in `discover`
  because no `*IDN?` is requested. USB discovery fills those fields from USB
  descriptors. Enriching network entries later must be an explicit, bounded
  step rather than part of the scan.
* Instruments whose portmapper answers only on TCP would be missed by the scan
  fallback. The (now working) broadcast `PMAP_DUMP` and the direct SCPI probe
  remain as other detection paths.
* A host that silently drops UDP to port 111 still costs one 300 ms deadline.
  The scan is bounded and concurrent, so this stays within budget.
