/**
 * Display-only screenshot profile matching.
 *
 * The authoritative profile table lives in Rust
 * (`crates/omariglol-core/src/screenshot.rs`) and is what actually performs a
 * capture. The frontend mirrors it so Screenshot mode can show the matched
 * profile name *before* the user captures, as the UX journey requires (§4.3).
 * Keep the prefixes in sync with `PROFILES` in the core.
 */

/** A profile as the UI needs it: a name and the query it would send. */
export interface DisplayProfile {
  name: string;
  /** Upper-cased `*IDN?` prefixes that select this profile. */
  idnPrefixes: string[];
  /** The capture query the core would use. */
  command: string;
}

const PROFILES: DisplayProfile[] = [
  {
    name: "Rigol DS1000Z",
    idnPrefixes: ["RIGOL TECHNOLOGIES,DS1"],
    command: ":DISP:DATA? ON,OFF,PNG",
  },
  {
    name: "Rigol DS2000",
    idnPrefixes: ["RIGOL TECHNOLOGIES,DS2"],
    command: ":DISP:DATA?",
  },
  {
    name: "Rigol DS4000",
    idnPrefixes: ["RIGOL TECHNOLOGIES,DS4"],
    command: ":DISP:DATA?",
  },
  {
    name: "Keysight InfiniiVision",
    idnPrefixes: ["KEYSIGHT TECHNOLOGIES,DSO"],
    command: ":DISP:DATA? PNG",
  },
  {
    name: "Rohde & Schwarz",
    idnPrefixes: ["ROHDE&SCHWARZ", "ROHDE & SCHWARZ"],
    command: ":HCOP:DEV:LANG PNG;:HCOP:DATA?",
  },
];

/** Pick the profile matching an `*IDN?` string, or `null`. */
export function matchProfile(idn: string): DisplayProfile | null {
  const upper = idn.trim().toUpperCase();
  for (const profile of PROFILES) {
    if (profile.idnPrefixes.some((prefix) => upper.startsWith(prefix))) {
      return profile;
    }
  }
  return null;
}
