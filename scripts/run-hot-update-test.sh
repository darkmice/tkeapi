#!/bin/bash
# tkeapi (tokensbyte) opensource
# © 2026 tkeapi.com
# @copyright      Copyright netbcloud/wstianxia
# @license        MIT (https://www.tkeapi.com/)

# TokensByte 热更新全自动压测与平滑切流验证运行器
# 用法:
#   ./scripts/run-hot-update-test.sh [--url http://127.0.0.1:3000] [--image docker.cnb.cool/...:preview | --file x.tar]

set -euo pipefail
cd "$(dirname "$0")/.."

URL="http://127.0.0.1:3000"
UPDATE_ARGS=()

while [ $# -gt 0 ]; do
    case "$1" in
        --url) URL="${2:-http://127.0.0.1:3000}"; shift 2 ;;
        *) UPDATE_ARGS+=("$1"); shift ;;
    esac
done

command -v python3 >/dev/null 2>&1 || { echo "❌ 请先安装 python3 以运行压测探针"; exit 1; }

echo "================================================================="
echo "       TokensByte 热更新压测套件启动"
echo "  目标服务: $URL"
echo "  执行更新参数: ${UPDATE_ARGS[*]:-(默认自动检测)}"
echo "================================================================="

PROBE_LOG="tmp_probe_result.json"

# 1. 启动背景流量探测器（守护模式）
echo "🚀 [Step 1] 正在启动流量探针（持续高并发 + 模拟流式请求）..."
python3 scripts/test-hot-update.py --url "$URL" --concurrency 20 --stream-concurrency 4 --qps 60 --output "$PROBE_LOG" --daemon &
PROBE_PID=$!

cleanup() {
    if kill -0 "$PROBE_PID" 2>/dev/null; then
        kill -INT "$PROBE_PID" 2>/dev/null || true
        wait "$PROBE_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM

# 等待 3 秒建立稳定的基线并发流量
sleep 3
echo "✅ [Step 2] 并发流量已稳定注入，开始执行热更新流程..."

# 2. 触发真正的热更新流程
if [ ${#UPDATE_ARGS[@]} -gt 0 ]; then
    ./hot-update.sh backend "${UPDATE_ARGS[@]}"
else
    ./hot-update.sh backend
fi

echo "✅ [Step 3] 热更新已完成切流！继续保持探测 5 秒验证新槽位稳定性..."
sleep 5

# 3. 停止流量探针并输出最终验收报告
echo "🏁 [Step 4] 停止流量探针并生成汇总报告..."
kill -INT "$PROBE_PID" 2>/dev/null || true
wait "$PROBE_PID" 2>/dev/null || true

echo "🎉 热更新自动化端到端压测执行完毕！"
