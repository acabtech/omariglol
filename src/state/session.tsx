/**
 * The global instrument session.
 *
 * The UX journey (§3.2) makes the session global, not per-mode: selecting an
 * instrument in Search makes it available to every other mode. The Rust side
 * owns the actual connection; this context mirrors it for the UI and is the
 * single place the title bar and the modes read session state from.
 */

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";

import * as api from "@/lib/api";
import { toCommandError } from "@/lib/api";
import { saveLastInstrument } from "@/lib/persist";
import type { CommandError, Transport } from "@/lib/types";

/** The active session, as the UI needs to describe it. */
export interface SessionInfo {
  /** The instrument's `*IDN?` string. */
  idn: string;
  address: string;
  transport: Transport;
  port?: number;
  timeoutSecs: number;
}

/** Arguments for opening a session. */
export interface ConnectArgs {
  address: string;
  transport: Transport;
  port?: number;
  timeoutSecs: number;
}

interface SessionContextValue {
  /** The active session, or `null` when nothing is connected. */
  session: SessionInfo | null;
  /** True while a connect is in flight. */
  connecting: boolean;
  /** The last connect error, or `null`. */
  error: CommandError | null;
  /** Open a session. Resolves to `true` on success. */
  connect: (args: ConnectArgs) => Promise<boolean>;
  /** Close the current session. */
  disconnect: () => Promise<void>;
  /** Clear the last connect error. */
  clearError: () => void;
}

const SessionContext = createContext<SessionContextValue | null>(null);

export function SessionProvider({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<SessionInfo | null>(null);
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);

  const connect = useCallback(async (args: ConnectArgs): Promise<boolean> => {
    setConnecting(true);
    setError(null);
    try {
      const idn = await api.connect(
        args.address,
        args.transport,
        args.port,
        args.timeoutSecs,
      );
      const info: SessionInfo = {
        idn,
        address: args.address,
        transport: args.transport,
        port: args.port,
        timeoutSecs: args.timeoutSecs,
      };
      setSession(info);
      // Remember it so Search can prefill the form next launch.
      saveLastInstrument({
        address: args.address,
        transport: args.transport,
        port: args.port,
        timeoutSecs: args.timeoutSecs,
        idn,
      });
      return true;
    } catch (err) {
      setError(toCommandError(err));
      return false;
    } finally {
      setConnecting(false);
    }
  }, []);

  const disconnect = useCallback(async () => {
    try {
      await api.disconnect();
    } catch (err) {
      // A failed disconnect still clears the UI session: the user asked to
      // stop working with the instrument, and the Rust side drops the link on
      // its own if the call failed.
      setError(toCommandError(err));
    } finally {
      setSession(null);
    }
  }, []);

  const clearError = useCallback(() => setError(null), []);

  const value = useMemo<SessionContextValue>(
    () => ({ session, connecting, error, connect, disconnect, clearError }),
    [session, connecting, error, connect, disconnect, clearError],
  );

  return (
    <SessionContext.Provider value={value}>{children}</SessionContext.Provider>
  );
}

/** Read the global session. Throws outside a `SessionProvider`. */
export function useSession(): SessionContextValue {
  const value = useContext(SessionContext);
  if (!value) {
    throw new Error("useSession must be used within a SessionProvider");
  }
  return value;
}
