/**
 * Timeline panel (concepts.md §2.5, §4.1).
 *
 * The shared automation timeline for the active workspace: every command,
 * query, error, preset, rule and trigger, in time order.
 */

import { useMemo } from "react";

import { Section } from "@/components/Section";
import { TimelineView } from "@/components/TimelineView";
import { useWorkspace } from "@/state/workspace";

export default function TimelineSection() {
  const { timeline, instruments, refreshTimeline } = useWorkspace();

  const labels = useMemo(() => {
    const map: Record<string, string> = {};
    for (const instrument of instruments) {
      map[instrument.id] = instrument.label;
    }
    return map;
  }, [instruments]);

  return (
    <Section
      id="timeline"
      title="Timeline"
      description="Every event across the workspace, for audit and replay."
    >
      <TimelineView
        events={timeline}
        instrumentLabels={labels}
        onRefresh={() => void refreshTimeline()}
      />
    </Section>
  );
}
