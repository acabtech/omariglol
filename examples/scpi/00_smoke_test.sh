#!/usr/bin/env sh
# 00_smoke_test.sh — exercise every basic DS1054Z function end to end.
#
# Runs each function in turn with a configurable delay between steps, printing
# PASS/FAIL and a summary. Unlike the other examples it does not stop at the
# first failure, so it doubles as a quick "is the instrument healthy?" check.
#
# Usage: ./00_smoke_test.sh [ADDRESS] [DELAY_SECONDS]
#
#   ADDRESS   instrument address (default: $RIGLOL_ADDRESS, then 192.168.100.36)
#   DELAY     seconds to wait between steps (default: $RIGLOL_DELAY, then 1)
#
# Environment: see common.sh (RIGLOL, RIGLOL_TRANSPORT, RIGLOL_RAW_PORT,
# RIGLOL_TIMEOUT, RIGLOL_DELAY).
# shellcheck source=common.sh
. "$(dirname "$0")/common.sh"

DELAY="${2:-${RIGLOL_DELAY:-1}}"

PASS=0
FAIL=0

# Run a labelled step, showing up to six lines of its output, then wait DELAY
# seconds. A failure is recorded but does not abort the run.
step() {
    label="$1"
    shift
    printf '%-30s ' "$label"
    if output=$("$@" 2>&1); then
        echo "PASS"
        PASS=$((PASS + 1))
    else
        echo "FAIL"
        FAIL=$((FAIL + 1))
    fi
    if [ -n "$output" ]; then
        printf '%s\n' "$output" | sed -n '1,6p' | sed 's/^/    /'
    fi
    sleep "$DELAY"
}

# Subcommand wrappers, so every step has the same shape.
screenshot_step() {
    tmp="$(mktemp)"
    "$RIGLOL_BIN" screenshot "$ADDRESS" "$tmp" --port "$RAW_PORT" --timeout "$TIMEOUT"
    rm -f "$tmp"
}

benchmark_step() {
    "$RIGLOL_BIN" benchmark "$ADDRESS" --count 5 --command "*IDN?" --raw --port "$RAW_PORT"
}

driver_step() {
    "$RIGLOL_BIN" driver "$ADDRESS" --raw --port "$RAW_PORT" --timeout "$TIMEOUT"
}

discover_step() {
    "$RIGLOL_BIN" discover --timeout 2
}

echo "== DS1054Z smoke test"
echo "   instrument: $ADDRESS"
echo "   delay:      ${DELAY}s between steps"
echo

step "identify (*IDN?)"    scpi "*IDN?"
step "clear status (*CLS)" scpi "*CLS"
step "reset (*RST)"        scpi "*RST"
step "self-test (*TST?)"   scpi "*TST?"
step "run (:RUN)"          scpi ":RUN"
step "stop (:STOP)"        scpi ":STOP"
step "single (:SING)"      scpi ":SING"
step "autoscale (:AUT)"    scpi ":AUT"
step "channel1 setup"      scpi ":CHAN1:DISP ON" ":CHAN1:COUP DC" ":CHAN1:PROB 10" ":CHAN1:SCAL 0.5" ":CHAN1:OFFS 0"
step "timebase"            scpi ":TIM:MODE MAIN" ":TIM:MAIN:SCAL 0.001" ":TIM:MAIN:OFFS 0"
step "acquire"             scpi ":ACQ:TYPE NORM" ":ACQ:MDEP AUTO"
step "trigger"             scpi ":TRIG:MODE EDGE" ":TRIG:EDGE:SOUR CHAN1" ":TRIG:EDGE:SLOP POS" ":TRIG:EDGE:LEV 0.5" ":TRIG:SWE AUTO"
step "math"                scpi ":MATH:OPER ADD" ":MATH:SOUR1 CHAN1" ":MATH:SOUR2 CHAN2" ":MATH:DISP ON"
step "measure"             scpi ":MEAS:SOUR CHAN1" ":MEAS:VPP?" ":MEAS:VAVG?" ":MEAS:VRMS?" ":MEAS:FREQ?" ":MEAS:PER?"
step "waveform"            scpi ":WAV:SOUR CHAN1" ":WAV:MODE NORM" ":WAV:FORM BYTE" ":WAV:POIN 1200" ":WAV:PRE?"
step "screenshot"          screenshot_step
step "benchmark"           benchmark_step
step "driver"              driver_step
step "discover"            discover_step

echo
echo "== summary: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
