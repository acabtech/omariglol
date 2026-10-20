/**
 * Shared UI primitives.
 *
 * These map onto the component inventory in UX journey §2.2. They carry no
 * colour of their own: every visual value resolves through an `--om-*` token
 * in `styles.css`.
 */

import type { ButtonHTMLAttributes, ReactNode } from "react";

import { cx } from "@/lib/cx";

/* -------------------------------------------------------------------------
 * Spinner
 * ---------------------------------------------------------------------- */

/** An indeterminate progress indicator. Hidden from assistive tech. */
export function Spinner() {
  return <span className="spinner" aria-hidden="true" />;
}

/* -------------------------------------------------------------------------
 * Button
 * ---------------------------------------------------------------------- */

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "default" | "primary" | "danger" | "ghost";
  /** Show a spinner and disable the button while an action runs. */
  busy?: boolean;
}

/**
 * A button with the three async states from §2.3: idle, busy (disabled with a
 * spinner) and settled. `busy` implies `disabled` and sets `aria-busy`.
 */
export function Button({
  variant = "default",
  busy = false,
  disabled,
  className,
  children,
  type = "button",
  ...rest
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cx(
        "button",
        variant !== "default" && `button--${variant}`,
        busy && "button--busy",
        className,
      )}
      disabled={disabled || busy}
      aria-busy={busy || undefined}
      {...rest}
    >
      {busy && <Spinner />}
      <span className="button__label">{children}</span>
    </button>
  );
}

/* -------------------------------------------------------------------------
 * Status dot
 * ---------------------------------------------------------------------- */

export type StatusState = "idle" | "ok" | "error";

/**
 * A connection/health indicator. Never conveys meaning by colour alone: it is
 * always paired with a text label by the caller (§6.7).
 */
export function StatusDot({ state }: { state: StatusState }) {
  return (
    <span
      className={cx(
        "status-dot",
        state === "ok" && "status-dot--ok",
        state === "error" && "status-dot--error",
      )}
      aria-hidden="true"
    />
  );
}

/* -------------------------------------------------------------------------
 * Alert
 * ---------------------------------------------------------------------- */

export type AlertTone = "info" | "warning" | "error" | "success";

const ALERT_ICON: Record<AlertTone, string> = {
  info: "i",
  warning: "!",
  error: "✕",
  success: "✓",
};

export interface AlertProps {
  tone?: AlertTone;
  title?: string;
  children?: ReactNode;
  /** When provided, renders a dismiss control. */
  onDismiss?: () => void;
}

/**
 * An inline message. Errors are never colour-only: each tone carries an icon
 * and, for errors, `role="alert"` so it is announced immediately.
 */
export function Alert({ tone = "info", title, children, onDismiss }: AlertProps) {
  return (
    <div
      className={cx("alert", `alert--${tone}`)}
      role={tone === "error" ? "alert" : "status"}
    >
      <span className="alert__icon" aria-hidden="true">
        {ALERT_ICON[tone]}
      </span>
      <div className="alert__body">
        {title && <strong className="alert__title">{title}</strong>}
        {children && <div className="alert__message">{children}</div>}
      </div>
      {onDismiss && (
        <button
          type="button"
          className="alert__dismiss"
          onClick={onDismiss}
          aria-label="Dismiss"
        >
          ×
        </button>
      )}
    </div>
  );
}

/* -------------------------------------------------------------------------
 * Empty state
 * ---------------------------------------------------------------------- */

export interface EmptyStateProps {
  title: string;
  children?: ReactNode;
  /** A next action, so an empty view is never a dead end (§1.1). */
  action?: ReactNode;
}

/** Guidance plus a next action, shown when a region has nothing to display. */
export function EmptyState({ title, children, action }: EmptyStateProps) {
  return (
    <div className="empty-state">
      <p className="empty-state__title">{title}</p>
      {children && <p className="empty-state__body">{children}</p>}
      {action && <div className="empty-state__action">{action}</div>}
    </div>
  );
}

/* -------------------------------------------------------------------------
 * Progress bar
 * ---------------------------------------------------------------------- */

export interface ProgressBarProps {
  value: number;
  max: number;
  label: string;
}

/** A determinate progress bar with the `progressbar` role (§6.4). */
export function ProgressBar({ value, max, label }: ProgressBarProps) {
  const percent = max > 0 ? Math.min(100, Math.max(0, (value / max) * 100)) : 0;
  return (
    <div
      className="progress"
      role="progressbar"
      aria-label={label}
      aria-valuenow={value}
      aria-valuemin={0}
      aria-valuemax={max}
    >
      <div className="progress__bar" style={{ width: `${percent}%` }} />
    </div>
  );
}

/* -------------------------------------------------------------------------
 * Disclosure
 * ---------------------------------------------------------------------- */

export interface DisclosureProps {
  summary: string;
  children: ReactNode;
  defaultOpen?: boolean;
}

/** Progressive disclosure (§3.6): a simple default that reveals depth. */
export function Disclosure({
  summary,
  children,
  defaultOpen = false,
}: DisclosureProps) {
  return (
    <details className="disclosure" open={defaultOpen}>
      <summary className="disclosure__summary">{summary}</summary>
      <div className="disclosure__body">{children}</div>
    </details>
  );
}

/* -------------------------------------------------------------------------
 * Badge
 * ---------------------------------------------------------------------- */

export interface BadgeProps {
  tone?: "neutral" | "ok" | "warning" | "error";
  children: ReactNode;
}

/** A small status label, e.g. a discovery method or a partial-run marker. */
export function Badge({ tone = "neutral", children }: BadgeProps) {
  return <span className={cx("badge", `badge--${tone}`)}>{children}</span>;
}

/* -------------------------------------------------------------------------
 * Connect prompt
 * ---------------------------------------------------------------------- */

/**
 * The compact "no instrument connected" affordance a session-bound section
 * shows inline, with a shortcut to the Connection section rather than a dead
 * end (§3.3). It is a single line, not a full empty state, so it does not
 * dominate the workbench.
 */
export function ConnectPrompt({
  onGoToConnection,
}: {
  onGoToConnection: () => void;
}) {
  return (
    <div className="connect-prompt">
      <span className="connect-prompt__text">No instrument connected.</span>
      <Button onClick={onGoToConnection}>Connect an instrument</Button>
    </div>
  );
}
