/**
 * Screenshot panel (UX journey §4.3, §6.3).
 *
 * Captures the selected instrument's display. The profile is matched from the
 * instrument's `*IDN?` so the user sees what will be captured before they
 * capture; an unknown instrument reveals a manual command override.
 */

import { useCallback, useEffect, useMemo, useState } from "react";

import { Field, TextInput } from "@/components/Field";
import { Section } from "@/components/Section";
import { Alert, Button, ConnectPrompt, Disclosure } from "@/components/ui";
import { useAsyncAction } from "@/hooks/useAsyncAction";
import * as api from "@/lib/api";
import { translateError } from "@/lib/errors";
import { fileTimestamp, idnLabel } from "@/lib/format";
import { matchProfile } from "@/lib/profiles";
import type { PanelProps, Screenshot } from "@/lib/types";
import { useWorkspace } from "@/state/workspace";

const MIME: Record<Screenshot["format"], string> = {
  png: "image/png",
  bmp: "image/bmp",
  jpeg: "image/jpeg",
  unknown: "application/octet-stream",
};

const EXTENSION: Record<Screenshot["format"], string> = {
  png: "png",
  bmp: "bmp",
  jpeg: "jpg",
  unknown: "bin",
};

/** Save a capture by triggering a download of its data URL. */
function saveScreenshot(shot: Screenshot): void {
  const url = `data:${MIME[shot.format]};base64,${shot.data}`;
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `${idnLabel(shot.idn)}-${fileTimestamp()}.${EXTENSION[shot.format]}`;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
}

export default function ScreenshotSection({ onGoToTopology }: PanelProps) {
  const { selected, refreshTimeline } = useWorkspace();
  const connected = selected?.state === "connected";

  const profile = useMemo(
    () => (selected?.idn ? matchProfile(selected.idn) : null),
    [selected?.idn],
  );

  const [overrideCommand, setOverrideCommand] = useState("");
  const [overrideOpen, setOverrideOpen] = useState(false);

  const capture = useAsyncAction(
    useCallback(
      (command?: string) =>
        api.captureScreenshot(command, selected?.id),
      [selected?.id],
    ),
  );

  // An unknown profile reveals the override by default (§5.3).
  useEffect(() => {
    setOverrideOpen(profile === null);
  }, [profile]);

  // A new instrument invalidates any previous capture.
  useEffect(() => {
    capture.reset();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selected?.id]);

  const shot = capture.result;

  const runCapture = useCallback(() => {
    const command = overrideCommand.trim();
    void capture.run(command === "" ? undefined : command).then(() => {
      void refreshTimeline();
    });
  }, [capture, overrideCommand, refreshTimeline]);

  // Ctrl+S saves when a capture is present (§3.8).
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
        if (shot) {
          event.preventDefault();
          saveScreenshot(shot);
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [shot]);

  const errorText = capture.error ? translateError(capture.error) : null;

  return (
    <Section
      id="screenshot"
      title="Screenshot"
      description={
        selected
          ? `Capture ${selected.label}'s display.`
          : "Capture the instrument's display."
      }
      actions={
        <Button
          variant="primary"
          busy={capture.busy}
          disabled={!connected}
          onClick={runCapture}
        >
          Capture
        </Button>
      }
    >
      {connected ? (
        <>
          {errorText && (
            <Alert tone="error" title="Capture failed">
              {errorText.message}
              {errorText.recovery && (
                <span className="alert__recovery"> {errorText.recovery}</span>
              )}
            </Alert>
          )}

          <div className="screenshot">
            <div className="screenshot__preview">
              {shot ? (
                <figure className="preview">
                  <img
                    className="preview__image"
                    src={`data:${MIME[shot.format]};base64,${shot.data}`}
                    alt={`${shot.profile} display from ${shot.idn}`}
                  />
                  <figcaption className="preview__caption mono">
                    {shot.profile} · {shot.format.toUpperCase()}
                  </figcaption>
                </figure>
              ) : (
                <div className="preview preview--empty">
                  <span className="preview__placeholder">
                    {capture.busy ? "reading image block…" : "No capture yet."}
                  </span>
                </div>
              )}
            </div>

            <div className="screenshot__side">
              <p className="meta-line">
                profile:{" "}
                <span className="mono">
                  {profile ? profile.name : "no built-in profile"}
                </span>
              </p>

              <Disclosure
                summary="manual command override"
                defaultOpen={overrideOpen}
              >
                {!profile && (
                  <p className="field__help">
                    This instrument has no built-in profile. Enter the capture
                    query.
                  </p>
                )}
                <Field label="Capture query" htmlFor="screenshot-command">
                  <TextInput
                    id="screenshot-command"
                    className="mono"
                    placeholder={profile?.command ?? ":DISP:DATA?"}
                    value={overrideCommand}
                    onChange={(event) => setOverrideCommand(event.target.value)}
                  />
                </Field>
              </Disclosure>

              <div className="action-row">
                <Button
                  variant="primary"
                  disabled={!shot}
                  onClick={() => shot && saveScreenshot(shot)}
                >
                  Save
                </Button>
                <span className="action-row__hint">Ctrl+S</span>
              </div>
            </div>
          </div>
        </>
      ) : (
        <ConnectPrompt onGoToConnection={onGoToTopology} />
      )}
    </Section>
  );
}
