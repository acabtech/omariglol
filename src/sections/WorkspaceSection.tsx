/**
 * Workspace manager (concepts.md §2.4).
 *
 * The single source of truth for "what is currently being worked on": create,
 * switch and delete workspaces, and manage the instruments within the active
 * one. This replaces the single-instrument Connection section.
 */

import { useCallback, useState } from "react";

import { Field, Select, TextInput } from "@/components/Field";
import { Section } from "@/components/Section";
import { Alert, Badge, Button, EmptyState } from "@/components/ui";
import { statusFor, stateLabel } from "@/components/TopologyView";
import { StatusDot } from "@/components/ui";
import { translateError } from "@/lib/errors";
import { useWorkspace } from "@/state/workspace";

export default function WorkspaceSection() {
  const {
    workspace,
    workspaces,
    instruments,
    selectedId,
    select,
    loading,
    error,
    clearError,
    createWorkspace,
    openWorkspace,
    saveWorkspace,
    deleteWorkspace,
    connectInstrument,
    disconnectInstrument,
    removeInstrument,
  } = useWorkspace();

  const [newName, setNewName] = useState("");
  const [busyId, setBusyId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const onCreate = useCallback(async () => {
    const name = newName.trim();
    if (!name) return;
    await createWorkspace(name);
    setNewName("");
  }, [createWorkspace, newName]);

  const onSave = useCallback(async () => {
    if (!workspace) return;
    setSaving(true);
    await saveWorkspace();
    setSaving(false);
  }, [saveWorkspace, workspace]);

  const onConnect = useCallback(
    async (id: string) => {
      setBusyId(id);
      await connectInstrument(id);
      setBusyId(null);
    },
    [connectInstrument],
  );

  const onDisconnect = useCallback(
    async (id: string) => {
      setBusyId(id);
      await disconnectInstrument(id);
      setBusyId(null);
    },
    [disconnectInstrument],
  );

  const errorText = error ? translateError(error) : null;

  return (
    <Section
      id="workspace"
      title="Workspace"
      description="The unit of work: a named collection of instruments and automation."
      actions={
        <Button onClick={() => void onSave()} busy={saving} disabled={!workspace}>
          Save
        </Button>
      }
    >
      {errorText && (
        <Alert tone="error" title="Workspace error" onDismiss={clearError}>
          {errorText.message}
        </Alert>
      )}

      <div className="form-grid">
        <Field label="Active workspace" htmlFor="workspace-select">
          <Select
            id="workspace-select"
            value={workspace?.id ?? ""}
            disabled={loading || workspaces.length === 0}
            onChange={(event) => void openWorkspace(event.target.value)}
          >
            {workspaces.length === 0 && <option value="">No workspaces</option>}
            {workspaces.map((summary) => (
              <option key={summary.id} value={summary.id}>
                {summary.name} ({summary.instrument_count})
              </option>
            ))}
          </Select>
        </Field>
        <Field label="New workspace" htmlFor="workspace-name">
          <TextInput
            id="workspace-name"
            placeholder="Frequency Sweep Feedback"
            value={newName}
            onChange={(event) => setNewName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void onCreate();
            }}
          />
        </Field>
      </div>

      <div className="action-row">
        <Button
          variant="primary"
          disabled={newName.trim() === ""}
          onClick={() => void onCreate()}
        >
          Create
        </Button>
        <Button
          variant="danger"
          disabled={!workspace}
          onClick={() => workspace && void deleteWorkspace(workspace.id)}
        >
          Delete workspace
        </Button>
        {workspace && (
          <span className="action-row__hint">
            {workspace.instruments.length} instruments ·{" "}
            {workspace.timeline_len} timeline events
          </span>
        )}
      </div>

      <div className="divider" />

      <div className="subgroup">
        <h3 className="subgroup__title">Instruments</h3>
        {instruments.length === 0 ? (
          <EmptyState title="No instruments yet">
            Add one from the Discovery panel, or connect directly there.
          </EmptyState>
        ) : (
          <div className="table-wrap">
            <table className="table table--selectable">
              <thead>
                <tr>
                  <th scope="col">Instrument</th>
                  <th scope="col">Address</th>
                  <th scope="col">Type</th>
                  <th scope="col">State</th>
                  <th scope="col">Actions</th>
                </tr>
              </thead>
              <tbody>
                {instruments.map((instrument) => {
                  const isSelected = instrument.id === selectedId;
                  const connected = instrument.state === "connected";
                  return (
                    <tr
                      key={instrument.id}
                      tabIndex={0}
                      aria-selected={isSelected}
                      className={isSelected ? "is-selected" : undefined}
                      onClick={() => select(instrument.id)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter" || event.key === " ") {
                          event.preventDefault();
                          select(instrument.id);
                        }
                      }}
                    >
                      <td>{instrument.label}</td>
                      <td className="mono">
                        {instrument.address}
                        {instrument.port != null ? `:${instrument.port}` : ""}
                      </td>
                      <td>
                        <Badge>{instrument.instrument_type}</Badge>
                      </td>
                      <td>
                        <span className="state-cell">
                          <StatusDot state={statusFor(instrument.state)} />
                          {stateLabel(instrument.state)}
                        </span>
                      </td>
                      <td>
                        <span className="row-actions">
                          {connected ? (
                            <Button
                              variant="danger"
                              busy={busyId === instrument.id}
                              onClick={(event) => {
                                event.stopPropagation();
                                void onDisconnect(instrument.id);
                              }}
                            >
                              Disconnect
                            </Button>
                          ) : (
                            <Button
                              variant="primary"
                              busy={busyId === instrument.id}
                              onClick={(event) => {
                                event.stopPropagation();
                                void onConnect(instrument.id);
                              }}
                            >
                              Connect
                            </Button>
                          )}
                          <Button
                            variant="ghost"
                            onClick={(event) => {
                              event.stopPropagation();
                              void removeInstrument(instrument.id);
                            }}
                          >
                            Remove
                          </Button>
                        </span>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </Section>
  );
}
