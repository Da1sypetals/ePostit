#!/usr/bin/env bash
# 全局热键真实端到端测试,对用户零干扰:
#   第二实例以 EPOSTIT_DATA_DIR 独立数据目录运行,配置、笔记、
#   日志与正式实例完全隔离,正式实例保持运行不受影响;
#   窗口按 EPOSTIT_DISPLAY_ID 创建在不可见虚拟显示器上,
#   EPOSTIT_NO_ACTIVATE 使唤起只排序窗口,应用永不取得前台焦点;
#   测试热键 cmd+; 只被测试实例注册,按键不会落到用户应用。
#   每轮: System Events keystroke 发送真实组合键(与物理按键同路
#   触发 Carbon 热键)唤起 -> CGWindowList 校验窗口在虚拟显示器内
#   出现 -> 再按一次走 request_hide 完整路径隐藏
# 用法: scripts/hotkey_e2e.sh [循环次数,默认 15]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
E2E="$ROOT/target/e2e"
HELPER="$E2E/helper"
VDHELPER="$E2E/vdisplay-helper"
APP="/Applications/ePostit.app/Contents/MacOS/ePostit"
DATA="$E2E/data"
LOG="$DATA/epostit.log"
CYCLES="${1:-15}"
APP_PID=""
VD_PID=""

mkdir -p "$E2E"
if [ ! -x "$HELPER" ] || [ "$ROOT/scripts/e2e_helper.swift" -nt "$HELPER" ]; then
    swiftc -O "$ROOT/scripts/e2e_helper.swift" -o "$HELPER"
fi
if [ ! -x "$VDHELPER" ] || [ "$ROOT/scripts/vdisplay_helper.m" -nt "$VDHELPER" ]; then
    clang -O2 -framework CoreGraphics -framework Foundation \
        "$ROOT/scripts/vdisplay_helper.m" -o "$VDHELPER"
fi
[ -x "$APP" ] || { echo "FAIL: $APP missing"; exit 1; }

cleanup() {
    [ -n "$APP_PID" ] && kill "$APP_PID" 2>/dev/null || true
    [ -n "$VD_PID" ] && kill "$VD_PID" 2>/dev/null || true
}
trap cleanup EXIT

rm -rf "$DATA"
mkdir -p "$DATA"

# 虚拟显示器位于主屏上方,窗口在其上不可见
"$VDHELPER" >"$E2E/vd.info" 2>"$E2E/vd.err" &
VD_PID=$!
for _ in $(seq 1 40); do
    [ -s "$E2E/vd.info" ] && break
    sleep 0.1
done
[ -s "$E2E/vd.info" ] || { echo "FAIL: virtual display not created"; cat "$E2E/vd.err" >&2; exit 1; }
read -r VDID VDX VDY VDW VDH _ _ _ _ <"$E2E/vd.info"
echo "virtual display $VDID at $VDX,$VDY ${VDW}x${VDH}"

"$HELPER" writeconfig "$DATA"
EPOSTIT_DATA_DIR="$DATA" EPOSTIT_DISPLAY_ID="$VDID" \
    EPOSTIT_NO_ACTIVATE=1 EPOSTIT_START_HIDDEN=1 \
    "$APP" &
APP_PID=$!

wait_log() {
    local pattern="$1" i
    for i in $(seq 1 60); do
        grep -qF "$pattern" "$LOG" 2>/dev/null && return 0
        sleep 0.1
    done
    return 1
}
wait_log_count() {
    local pattern="$1" want="$2" i count
    for i in $(seq 1 60); do
        count=$(grep -cF "$pattern" "$LOG" 2>/dev/null || true)
        [ "${count:-0}" -ge "$want" ] && return 0
        sleep 0.1
    done
    return 1
}
wait_window() {
    local want="$1" i info
    for i in $(seq 1 160); do
        info="$("$HELPER" onscreen "$APP_PID")"
        [ "${info%% *}" = "$want" ] && { echo "$info"; return 0; }
        sleep 0.025
    done
    return 1
}
check_in_vdisplay() {
    local x="$1" y="$2" w="$3" h="$4"
    [ "$x" -ge "$VDX" ] && [ "$y" -ge "$VDY" ] \
        && [ $((x + w)) -le $((VDX + VDW)) ] && [ $((y + h)) -le $((VDY + VDH)) ]
}
assert_not_frontmost() {
    [ "$("$HELPER" frontmost)" != "$APP_PID" ] \
        || { echo "FAIL: test instance stole foreground focus"; exit 1; }
}
send_key() {
    osascript -e 'tell application "System Events" to keystroke ";" using {command down}'
}

wait_log "hotkey install" || { echo "FAIL: hotkey install not logged"; exit 1; }
wait_log "registered" || { echo "FAIL: cold start hotkey not registered"; exit 1; }
wait_window 0 >/dev/null || { echo "FAIL: window onscreen at hidden start"; exit 1; }
assert_not_frontmost
echo "cold start: hidden launch, hotkey registered"

for i in $(seq 1 "$CYCLES"); do
    send_key
    wait_log_count "hotkey event" $((i * 2 - 1)) \
        || { echo "FAIL: cycle $i, hotkey event not delivered"; exit 1; }
    info=$(wait_window 1) || { echo "FAIL: cycle $i, hotkey did not wake the window"; exit 1; }
    read -r _ WX WY WW WH <<<"$info"
    check_in_vdisplay "$WX" "$WY" "$WW" "$WH" \
        || { echo "FAIL: cycle $i, window outside virtual display at $info"; exit 1; }
    assert_not_frontmost
    send_key
    wait_log_count "hotkey event" $((i * 2)) \
        || { echo "FAIL: cycle $i, second hotkey event not delivered"; exit 1; }
    wait_window 0 >/dev/null || { echo "FAIL: cycle $i, hotkey did not hide the window"; exit 1; }
    wait_log_count "request_hide" "$i" || { echo "FAIL: cycle $i, request_hide not logged"; exit 1; }
    assert_not_frontmost
    echo "cycle $i ok"
done

echo "PASS: $CYCLES keystroke wake + hide cycles, zero focus steals"
