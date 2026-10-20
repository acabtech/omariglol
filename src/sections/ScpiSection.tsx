/**
 * SCPI console (UX journey §4.2, §6.2; concepts.md §2.5).
 *
 * A command console bound to the selected instrument. Commands ending in `?`
 * are queries; everything else is a write. History recall and a hex toggle for
 * binary responses are preserved from 0.1.0.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import { Console, type ConsoleLine } from "@/components/Console";
import { Checkbox } from "@/components/Field";
import { Section } from "@/components/Section";
import { Button, ConnectPrompt } from "@/components/ui";
import * as api from "@/lib/api";
import { toCommandError } from "@/lib/api";
import { translateError } from "@/lib/errors";
import { byteLength, hasNonPrintable, toHexDump } from "@/lib/format";
import type { PanelProps } from "@/lib/types";
import { useWorkspace } from "@/state/workspace";

export default function ScpiSection({ onGoToTopology }: PanelProps) {
  const { selected, refreshTimeline } = useWorkspace();
  const connected = selected?.state === "connected";

  const [lines, setLines] = useState<ConsoleLine[]>([]);
  const [input, setInput] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);
  const [hex, setHex] = useState(false);
  const [sending, setSending] = useState(false);

  const inputRef = useRef<HTMLInputElement>(null);
  const nextId = useRef(0);

  const pushLine = useCallback((text: string, tone?: ConsoleLine["tone"]) => {
    setLines((prev) => [...prev, { id: nextId.current++, text, tone }]);
  }, []);

  // Ctrl+L focuses the command input (§3.8).
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "l") {
        event.preventDefault();
        inputRef.current?.focus();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  // A new instrument invalidates the console.
  useEffect(() => {
    setLines([]);
  }, [selected?.id]);

  const send = useCallback(async () => {
    const command = input.trim();
    if (!command || sending || !selected) return;

    pushLine(`> ${command}`, "echo");
    setInput("");
    setHistory((prev) => [...prev, command]);
    setHistoryIndex(null);
    setSending(true);

    try {
      if (command.endsWith("?")) {
        const response = await api.instrumentQuery(selected.id, command);
        if (hex) {
          pushLine(toHexDump(response));
        } else if (hasNonPrintable(response)) {
          pushLine(
            `binary response (${byteLength(response)} bytes) — enable hex to view`,
            "muted",
          );
        } else {
          pushLine(response);
        }
      } else {
        await api.instrumentWrite(selected.id, command);
      }
      await refreshTimeline();
    } catch (err) {
      const translated = translateError(toCommandError(err));
      pushLine(`error: ${translated.message}`, "error");
    } finally {
      setSending(false);
      inputRef.current?.focus();
    }
  }, [hex, input, pushLine, selected, sending, refreshTimeline]);

  const navigateHistory = useCallback(
    (delta: number) => {
      if (history.length === 0) return;
      const current = historyIndex === null ? history.length : historyIndex;
      const next = Math.min(history.length, Math.max(0, current + delta));
      if (next === history.length) {
        setHistoryIndex(null);
        setInput("");
      } else {
        setHistoryIndex(next);
        setInput(history[next]);
      }
    },
    [history, historyIndex],
  );

  const onInputKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      void send();
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      navigateHistory(-1);
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      navigateHistory(1);
    }
  };

  return (
    <Section
      id="scpi"
      title="SCPI Console"
      description={
        selected
          ? `Send commands to ${selected.label} and read responses.`
          : "Send commands and read responses."
      }
      actions={
        <Button onClick={() => setLines([])} disabled={lines.length === 0}>
          Clear
        </Button>
      }
    >
      {connected ? (
        <>
          <Console
            lines={lines}
            label="SCPI console"
            empty="Send a command to see its response here."
          />
          <div className="input-row">
            <label className="visually-hidden" htmlFor="scpi-command">
              SCPI command
            </label>
            <input
              id="scpi-command"
              ref={inputRef}
              className="input input--command"
              placeholder="*IDN?"
              autoComplete="off"
              spellCheck={false}
              value={input}
              disabled={sending}
              onChange={(event) => setInput(event.target.value)}
              onKeyDown={onInputKeyDown}
            />
            <Button variant="primary" busy={sending} onClick={() => void send()}>
              Send
            </Button>
            <Checkbox
              id="scpi-hex"
              label="hex"
              checked={hex}
              onChange={(event) => setHex(event.target.checked)}
            />
          </div>
        </>
      ) : (
        <ConnectPrompt onGoToConnection={onGoToTopology} />
      )}
    </Section>
  );
}
