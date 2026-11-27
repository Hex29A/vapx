#!/usr/bin/env bash
# fw-compat.sh — firmware compatibility smoke test for one camera.
#
#   scripts/fw-compat.sh [--write] [--out DIR] [--vapx PATH] <camera>
#
# <camera> is a name from cameras.yaml (or an address that vapx resolves the
# same way). The script reads the firmware version first and applies the
# support levels:
#
#   AXIS OS 10.12 and later  full: read phase, and the write phase with --write
#   AXIS OS 8.40 to 10.11    reading only: --write is refused (8.40 to 9.80 is
#                            the supported range; 10.0 to 10.11 is untested)
#   older than 8.40          not supported: nothing is run, exit 2
#
# Read phase: a fixed list of read-only commands. Write phase (--write): a
# parameter backup first, then reversible tests (parameter, overlay, user
# account, SSH switch) that each add or change something, read it back and
# undo it, and finally a second backup compared with the first. The camera
# must end up as it started.
#
# Every command's output is kept in DIR (default: fw-compat-<camera>-<time>),
# with results.tsv (one row per command) and column.md (the column for the
# compatibility table). The outputs hold serial numbers and addresses: keep
# them out of public places.
#
# A command passes when it exits 0. A clear "needs AXIS OS X" or "not
# available on this camera" answer counts as n/a, which is also a pass. Anything
# else fails, including a raw HTTP 404 or a VAPIX error about the API version.
# Exit code: 0 when nothing failed, 1 when something did, 2 when the camera
# could not be tested at all.
#
# Needs: bash, jq, and vapx in PATH (or --vapx).

set -u

usage() { sed -n '2,32p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }

WRITE=0
OUT=""
VAPX="${VAPX:-vapx}"
CAM=""
while [ $# -gt 0 ]; do
    case "$1" in
        --write) WRITE=1 ;;
        --out) OUT="${2:?--out needs a directory}"; shift ;;
        --vapx) VAPX="${2:?--vapx needs a path}"; shift ;;
        -h|--help) usage ;;
        -*) echo "fw-compat: unknown option $1" >&2; exit 2 ;;
        *) [ -z "$CAM" ] || { echo "fw-compat: one camera at a time" >&2; exit 2; }; CAM="$1" ;;
    esac
    shift
done
[ -n "$CAM" ] || usage
command -v jq >/dev/null || { echo "fw-compat: needs jq" >&2; exit 2; }
command -v "$VAPX" >/dev/null || { echo "fw-compat: vapx not found ($VAPX)" >&2; exit 2; }

SAFE_CAM=$(printf '%s' "$CAM" | tr -c 'A-Za-z0-9._-' '_')
OUT="${OUT:-fw-compat-$SAFE_CAM-$(date +%Y%m%d-%H%M)}"
mkdir -p "$OUT" && chmod 700 "$OUT" || exit 2
RESULTS="$OUT/results.tsv"
printf 'label\tresult\texit\tseconds\tnote\n' > "$RESULTS"
PASS=0; NA=0; FAIL=0; SKIP=0

# The error message: `message` from vapx's JSON error, else the first line.
first_line() {
    local msg
    msg=$(jq -r '.message? // empty' "$1" 2>/dev/null)
    [ -n "$msg" ] || msg=$(grep -v '^\s*$' "$1" | head -1)
    printf '%s' "$msg" | head -1 | cut -c1-160 | tr '\t' ' '
}

# run LABEL ARGS... — run one vapx command and record the result.
run() {
    local label="$1"; shift
    local start=$SECONDS code note result
    "$VAPX" "$@" >"$OUT/$label.out" 2>"$OUT/$label.err" </dev/null
    code=$?
    if [ $code -eq 0 ]; then
        result=pass; note=""; PASS=$((PASS + 1))
    elif grep -qiE 'requires AXIS OS|needs AXIS OS|not available on this|not supported (on|by) this' "$OUT/$label.err"; then
        result=n/a; note=$(first_line "$OUT/$label.err"); NA=$((NA + 1))
    else
        result=FAIL; note=$(first_line "$OUT/$label.err"); FAIL=$((FAIL + 1))
    fi
    printf '%s\t%s\t%s\t%s\t%s\n' "$label" "$result" "$code" "$((SECONDS - start))" "$note" >> "$RESULTS"
    printf '%-22s %-5s %s\n' "$label" "$result" "$note"
    return $code
}

skip() {
    SKIP=$((SKIP + 1))
    printf '%s\tskip\t-\t0\t%s\n' "$1" "$2" >> "$RESULTS"
    printf '%-22s %-5s %s\n' "$1" skip "$2"
}

# --- firmware and support level ---

if ! "$VAPX" info "$CAM" >"$OUT/info.out" 2>"$OUT/info.err" </dev/null; then
    echo "fw-compat: cannot read the firmware version: $(first_line "$OUT/info.err")" >&2
    exit 2
fi
VERSION=$(jq -r '[.. | objects | .Version? // empty] | first // empty' "$OUT/info.out")
MODEL=$(jq -r '[.. | objects | .ProdNbr? // empty] | first // empty' "$OUT/info.out")
[ -n "$VERSION" ] || { echo "fw-compat: no firmware version in the info output" >&2; exit 2; }

# version_ge A B — A >= B, comparing dotted numbers.
version_ge() { [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -1)" = "$2" ]; }

if version_ge "$VERSION" 10.12; then
    LEVEL=full
elif version_ge "$VERSION" 8.40; then
    LEVEL=read-only
else
    LEVEL=unsupported
fi

echo "Camera $CAM: ${MODEL:-unknown model}, AXIS OS $VERSION, support level: $LEVEL"
echo "Output: $OUT"
{
    echo "camera=$CAM"; echo "model=$MODEL"; echo "version=$VERSION"; echo "level=$LEVEL"
    echo "vapx=$("$VAPX" --version 2>/dev/null)"; echo "date=$(date '+%Y-%m-%d %H:%M')"
} > "$OUT/meta.txt"

if [ "$LEVEL" = unsupported ]; then
    echo "fw-compat: AXIS OS $VERSION is older than 8.40, which vapx does not support. Nothing was run." >&2
    exit 2
fi
if [ $WRITE -eq 1 ] && [ "$LEVEL" != full ]; then
    echo "fw-compat: AXIS OS $VERSION is below 10.12, where vapx supports reading only; the write phase is not run." >&2
    WRITE=0
fi

# --- phase A: reading ---

echo; echo "Phase A: reading"
printf 'info\tpass\t0\t0\t\n' >> "$RESULTS"; PASS=$((PASS + 1))
run systemready      systemready "$CAM"
run fw_status        fw status "$CAM"
run fw_check         fw check "$CAM"
run discover         discover "$CAM"
run audit            audit "$CAM"
run health           health "$CAM"
run acap_list        acap list "$CAM"
run acap_unsigned    acap unsigned status "$CAM"
run user_list        user list "$CAM"
run user_list_all    user list "$CAM" --all
run ssh_status       ssh status "$CAM"
run net_show         net show "$CAM"
run time_show        time show "$CAM"
run hw_show          hw show "$CAM"
run log_system       log system "$CAM" --tail 20
run log_access       log access "$CAM"
run cert_list        cert list "$CAM"
run storage_list     storage list "$CAM"
run storage_health   storage health "$CAM"
run storage_params   storage params "$CAM"
run disk_status      disk status "$CAM"
run o3c              o3c status "$CAM"
run temp             temp "$CAM"
run daynight         daynight "$CAM"
run imaging          imaging "$CAM"
run light            light "$CAM"
run vmd              vmd "$CAM"
run audio            audio "$CAM"
run clip_list        clip list "$CAM"
run mqtt_status      mqtt status "$CAM"
run mqtt_events      mqtt events "$CAM"
run streamstatus     streamstatus "$CAM"
run signedvideo      signedvideo status "$CAM"
run zipstream        zipstream status "$CAM"
run viewarea         viewarea list "$CAM"
run rule_list        rule list "$CAM"
run overlay_list     overlay list "$CAM"
run overlay_caps     overlay capabilities "$CAM"
run ptz_position     ptz query "$CAM" position
run ptz_limits       ptz query "$CAM" limits
run snap             snap "$CAM" -o "$OUT/snap.jpg"
run backup           backup save "$CAM" -o "$OUT/baseline-params.json"

# --- phase C: writing and undoing ---

if [ $WRITE -eq 1 ]; then
    echo; echo "Phase C: writing and undoing"
    BASELINE="$OUT/write-baseline-params.json"
    if ! run write_baseline backup save "$CAM" -o "$BASELINE"; then
        echo "fw-compat: no baseline, so nothing is written" >&2
    else
        # A parameter: set, read back, restore.
        P=root.Image.I0.Text.String
        # `param get` prints the bare value, one line per parameter.
        if run param_get_before param get "$CAM" "$P" && [ -s "$OUT/param_get_before.out" ]; then
            OLD=$(head -1 "$OUT/param_get_before.out")
            run param_set param set "$CAM" "$P=vapx-compat"
            run param_get_after param get "$CAM" "$P"
            grep -qx vapx-compat "$OUT/param_get_after.out" || { FAIL=$((FAIL + 1)); printf 'param_readback\tFAIL\t-\t0\tvalue not read back\n' >> "$RESULTS"; }
            run param_restore param set "$CAM" "$P=$OLD"
        else
            skip param_set "$P not on this camera"
        fi

        # An overlay: add, find, remove.
        if run overlay_add overlay add "$CAM" --text vapx-compat; then
            ID=$(jq -r '[.. | objects | (.identity? // .id?) // empty] | first // empty' "$OUT/overlay_add.out")
            if [ -n "$ID" ]; then
                run overlay_remove overlay remove "$CAM" --id "$ID"
            else
                FAIL=$((FAIL + 1)); printf 'overlay_remove\tFAIL\t-\t0\tno identity in the add output, remove it by hand\n' >> "$RESULTS"
            fi
        fi

        # A viewer account with a throwaway password: add, list, remove.
        FW_COMPAT_PWD=$(LC_ALL=C tr -dc 'A-Za-z0-9' </dev/urandom | head -c 20)Aa1
        export FW_COMPAT_PWD
        if run user_add user add "$CAM" --name vapxcompat --role viewer --pwd-env FW_COMPAT_PWD; then
            run user_list_new user list "$CAM"
            grep -q vapxcompat "$OUT/user_list_new.out" || { FAIL=$((FAIL + 1)); printf 'user_readback\tFAIL\t-\t0\tnew account not listed\n' >> "$RESULTS"; }
            run user_remove user remove "$CAM" --name vapxcompat --yes
        fi
        unset FW_COMPAT_PWD

        # SSH: only switched on and off again when it is off.
        if run ssh_get param get "$CAM" root.Network.SSH.Enabled && grep -qx no "$OUT/ssh_get.out"; then
            run ssh_enable ssh enable "$CAM"
            run ssh_disable ssh disable "$CAM"
        else
            skip ssh_toggle "SSH is on or unknown; left alone"
        fi

        # The camera must be as it was.
        if run write_after backup save "$CAM" -o "$OUT/write-after-params.json"; then
            if diff <(jq -S . "$BASELINE") <(jq -S . "$OUT/write-after-params.json") > "$OUT/write-diff.txt"; then
                printf 'baseline_diff\tpass\t0\t0\t\n' >> "$RESULTS"; PASS=$((PASS + 1))
                echo "baseline_diff          pass"
            else
                printf 'baseline_diff\tFAIL\t1\t0\tsee write-diff.txt; restore with vapx backup restore\n' >> "$RESULTS"; FAIL=$((FAIL + 1))
                echo "baseline_diff          FAIL   parameters differ from the baseline, see $OUT/write-diff.txt"
            fi
        fi
    fi
fi

# --- the column ---

{
    echo "| $CAM ($MODEL, AXIS OS $VERSION, $LEVEL) | |"
    echo "|---|---|"
    tail -n +2 "$RESULTS" | while IFS=$'\t' read -r label result _ _ note; do
        echo "| $label | $result${note:+ — $note} |"
    done
} > "$OUT/column.md"

echo
echo "Passed $PASS, n/a $NA, failed $FAIL, skipped $SKIP. Column: $OUT/column.md"
[ $FAIL -eq 0 ]
