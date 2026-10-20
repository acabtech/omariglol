/**
 * Tiny class-name joiner.
 *
 * Keeps components free of a runtime dependency while still allowing the
 * conditional class lists the design system relies on (`button--primary`,
 * `status-dot--ok`, …). Falsy values are dropped.
 */
export function cx(
  ...values: Array<string | false | null | undefined>
): string {
  return values.filter(Boolean).join(" ");
}
