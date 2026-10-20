#!/bin/bash
# tkeapi (tokensbyte) opensource
# © 2026 tkeapi.com
# @copyright      Copyright netbcloud/wstianxia 
# @license        MIT (https://www.tkeapi.com/)

# ──────────────────────────────────────────────────
# TokensByte 本地开发启动脚本
# - 默认后台：拉起后端口就绪即退出，进程继续跑
# - 前台日志：./dev.sh fg 或 DEV_ATTACH=1，终端刷日志，Ctrl+C 停本实例
# - 多实例：按路径哈希隔离 state；共用 Postgres；前后端端口避让
# - 仅回收本仓库残留进程，不误杀其它目录实例
# 可选环境变量：
#   PROJECT_NAME / BACKEND_PORT / FRONTEND_PORT / POSTGRES_PORT
#   DATABASE_URL / DEV_MODE(1|2) / RUST_LOG / DEV_WAIT_MAX / DEV_ATTACH
#   TOKENSBYTE_FAST_LINK=0     关闭 Rust 链接加速（Linux mold/lld；macOS 可选 lld）
#   TOKENSBYTE_LOCAL_TARGET=0  强制不把 target 迁到本机盘（默认：仓库在 /Volumes 时自动迁移）
#   TOKENSBYTE_SCCACHE=0       关闭 sccache（默认：检测到 sccache 则启用）
#   TOKENSBYTE_AUTO_BREW=0     关闭自动 brew 安装 sccache（默认 macOS 缺则安装；不装巨型 llvm）
# 编译缓存：勿随意删除 backend/target 或本机 CARGO_TARGET_DIR，否则下次冷编译
# 用法：./dev.sh [1|2] [bg|fg]   1=本地(默认后台)  2=Docker 全容器
# ──────────────────────────────────────────────────
set -e

ROOT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
cd "${ROOT_DIR}"
export PATH="/opt/homebrew/bin:/usr/local/bin:/Applications/Docker.app/Contents/Resources/bin:${HOME}/.cargo/bin:${HOME}/.local/bin:${PATH}"

if ! command -v docker >/dev/null 2>&1 || ! docker --version >/dev/null 2>&1; then
    if [ -d "/Applications/Docker.app/Contents/Resources/bin" ]; then
        export PATH="/Applications/Docker.app/Contents/Resources/bin:${PATH}"
    fi
fi

if [ -f "${ROOT_DIR}/.env" ]; then
    set -a
    # shellcheck disable=SC1091
    source "${ROOT_DIR}/.env"
    set +a
fi

PROJECT_NAME=${PROJECT_NAME:-$(basename "$ROOT_DIR")}
POSTGRES_PORT=${POSTGRES_PORT:-5432}
POSTGRES_USER=${POSTGRES_USER:-tokensapi}
PREFERRED_BACKEND_PORT=${BACKEND_PORT:-3000}
PREFERRED_FRONTEND_PORT=${FRONTEND_PORT:-5173}
DEV_ATTACH="${DEV_ATTACH:-0}"


choice=""
for arg in "$@"; do
    case "${arg}" in
        1|2) choice="${arg}" ;;
        fg|foreground|attach|log) DEV_ATTACH=1 ;;
        bg|background|daemon) DEV_ATTACH=0 ;;
        -h|--help|help)
            echo "用法: ./dev.sh [1|2] [bg|fg]"
            echo "  1 / 默认  本地开发（默认 bg 后台）"
            echo "  2         Docker 全容器"
            echo "  fg        前台输出日志，Ctrl+C 停止本实例"
            echo "  bg        后台运行（默认）"
            echo ""
            echo "加速相关环境变量："
            echo "  TOKENSBYTE_FAST_LINK=0     关闭链接加速"
            echo "  TOKENSBYTE_LOCAL_TARGET=0  禁止将 target 迁到本机 SSD 缓存"
            echo "  TOKENSBYTE_SCCACHE=0       禁用 sccache（有则默认启用）"
            echo "  TOKENSBYTE_AUTO_BREW=0     禁用自动 brew install sccache"
            exit 0
            ;;
        *)
            echo "❌ 无效参数: ${arg}（可用: ./dev.sh [1|2] [bg|fg]）"
            exit 1
            ;;
    esac
done
choice="${choice:-${DEV_MODE:-1}}"

STATE_ID=$(printf '%s' "${ROOT_DIR}" | shasum -a 256 2>/dev/null | cut -c1-12)
[ -n "${STATE_ID}" ] || STATE_ID=$(printf '%s' "${ROOT_DIR}" | cksum | awk '{print $1}')
STATE_FILE="${TMPDIR:-/tmp}/tokensbyte-dev-${STATE_ID}.state"

port_in_use() {
    local port="$1"
    if command -v lsof >/dev/null 2>&1; then
        lsof -nP -iTCP:"${port}" -sTCP:LISTEN >/dev/null 2>&1
        return $?
    fi
    if command -v python3 >/dev/null 2>&1; then
        if python3 -c "import socket; s=socket.socket(); s.bind(('127.0.0.1', int('${port}'))); s.close()" >/dev/null 2>&1; then
            return 1
        fi
        return 0
    fi
    return 0
}

pick_free_port() {
    # 不可写成 local start=.. max=$((start+100))（同行 start 未生效，max 会变 100）
    local start="$1" label="$2" p="$1" max
    max=$((start + 100))
    while [ "${p}" -le "${max}" ]; do
        if ! port_in_use "${p}"; then
            [ "${p}" != "${start}" ] && echo "ℹ️  ${label} 端口 ${start} 已被占用，改用 ${p}" >&2
            echo "${p}"
            return 0
        fi
        p=$((p + 1))
    done
    echo "❌ 无法为 ${label} 找到可用端口（已尝试 ${start}-${max}）" >&2
    exit 1
}

hard_kill_pid() {
    local pid="$1"
    [ -n "${pid}" ] || return 0
    kill -0 "${pid}" 2>/dev/null || return 0
    kill -CONT "${pid}" 2>/dev/null || true
    kill -KILL "${pid}" 2>/dev/null || true
}

kill_tree() {
    local pid="$1" c
    [ -n "${pid}" ] || return 0
    kill -0 "${pid}" 2>/dev/null || return 0
    for c in $(pgrep -P "${pid}" 2>/dev/null || true); do
        kill_tree "${c}"
    done
    hard_kill_pid "${pid}"
}

free_listen_port() {
    local port="$1" pid
    [ -n "${port}" ] || return 0
    command -v lsof >/dev/null 2>&1 || return 0
    for pid in $(lsof -nP -tiTCP:"${port}" -sTCP:LISTEN 2>/dev/null || true); do
        hard_kill_pid "${pid}"
    done
}

# 双 fork 守护启动（macOS 无 setsid；避免脚本/Agent 会话退出后子进程被带走）
# 用法: daemonize_run <workdir> <logfile> -- <cmd> [args...]
daemonize_run() {
    local workdir="$1" logfile="$2"
    shift 2
    if [ "$1" != "--" ]; then
        echo "❌ daemonize_run 用法错误" >&2
        return 1
    fi
    shift
    python3 - "${workdir}" "${logfile}" "$@" <<'PY'
import os, sys
workdir, logfile = sys.argv[1], sys.argv[2]
cmd = sys.argv[3:]
if not cmd:
    raise SystemExit("empty command")
log = open(logfile, "ab", buffering=0)
if os.fork() > 0:
    raise SystemExit(0)
os.setsid()
if os.fork() > 0:
    raise SystemExit(0)
os.chdir(workdir)
os.dup2(log.fileno(), 1)
os.dup2(log.fileno(), 2)
try:
    os.close(log.fileno())
except Exception:
    pass
os.execvpe(cmd[0], cmd, os.environ)
PY
}

state_get() {
    [ -f "${STATE_FILE}" ] || return 0
    sed -n "s/^${1}=//p" "${STATE_FILE}" | head -n1
}

write_run_state() {
    printf 'BACKEND_PORT=%s\nFRONTEND_PORT=%s\nBACKEND_PID=%s\nFRONTEND_PID=%s\n' \
        "${BACKEND_PORT}" "${FRONTEND_PORT}" "${BACKEND_PID:-}" "${FRONTEND_PID:-}" > "${STATE_FILE}"
}

proc_cwd() {
    lsof -a -p "$1" -d cwd -Fn 2>/dev/null | sed -n 's/^n//p' | head -n1
}

kill_if_cwd() {
    local pid="$1" want="$2"
    [ -n "${pid}" ] && [ "${pid}" != "$$" ] || return 0
    [ "$(proc_cwd "${pid}")" = "${want}" ] || return 0
    hard_kill_pid "${pid}"
}

# Linux：mold / lld；macOS：若已安装 Homebrew llvm 的 ld64.lld 则启用
# macOS 缺依赖时可由 ensure_dev_brew_tooling 先 brew install
find_mac_lld() {
    local cand
    for cand in \
        /opt/homebrew/opt/llvm/bin/ld64.lld \
        /usr/local/opt/llvm/bin/ld64.lld \
        "$(command -v ld64.lld 2>/dev/null || true)"; do
        if [ -n "${cand}" ] && [ -x "${cand}" ]; then
            if echo 'int main(){return 0;}' | clang -x c - -fuse-ld="${cand}" -o /dev/null >/dev/null 2>&1; then
                printf '%s\n' "${cand}"
                return 0
            fi
        fi
    done
    return 1
}

# macOS：缺 sccache 时自动 brew 安装（TOKENSBYTE_AUTO_BREW=0 可关）
# 不自动装 llvm：整包约 GB 级，收益有限；若已手动 brew install llvm/lld 则仍启用链接加速
ensure_dev_brew_tooling() {
    [ "$(uname -s)" = "Darwin" ] || return 0
    [ "${TOKENSBYTE_AUTO_BREW:-1}" = "0" ] && return 0
    command -v brew >/dev/null 2>&1 || {
        echo "ℹ️  未找到 Homebrew，跳过自动安装 sccache"
        return 0
    }

    if [ "${TOKENSBYTE_SCCACHE:-1}" != "0" ] && ! command -v sccache >/dev/null 2>&1; then
        echo "📦 未找到 sccache，正在 brew install sccache ..."
        if brew install sccache; then
            # 刷新当前 shell 的 PATH（Apple Silicon 默认 /opt/homebrew/bin）
            if [ -x /opt/homebrew/bin/sccache ]; then
                export PATH="/opt/homebrew/bin:${PATH}"
            elif [ -x /usr/local/bin/sccache ]; then
                export PATH="/usr/local/bin:${PATH}"
            fi
            hash -r 2>/dev/null || true
            echo "✅ sccache 已安装: $(command -v sccache 2>/dev/null || echo 未在 PATH)"
        else
            echo "⚠️  brew install sccache 失败，将继续用不带缓存的编译"
        fi
    fi
}

apply_dev_rust_link_accel() {
    DEV_CARGO_WRAPPER=""
    if [ "${TOKENSBYTE_FAST_LINK:-1}" = "0" ]; then
        echo "ℹ️  已关闭 Rust 链接加速 (TOKENSBYTE_FAST_LINK=0)"
        return 0
    fi
    local os
    os="$(uname -s)"
    if [ "${os}" = "Linux" ]; then
        if command -v mold >/dev/null 2>&1; then
            DEV_CARGO_WRAPPER="mold -run"
            echo "ℹ️  Rust 链接加速: mold"
            return 0
        fi
        if command -v clang >/dev/null 2>&1 && command -v ld.lld >/dev/null 2>&1; then
            case " ${RUSTFLAGS:-} " in
                *" -fuse-ld="*) ;;
                *)
                    export RUSTFLAGS="${RUSTFLAGS:+${RUSTFLAGS} }-C linker=clang -C link-arg=-fuse-ld=lld"
                    echo "ℹ️  Rust 链接加速: lld"
                    ;;
            esac
            return 0
        fi
        return 0
    fi

    if [ "${os}" = "Darwin" ]; then
        local lld_bin=""
        lld_bin="$(find_mac_lld || true)"
        if [ -n "${lld_bin}" ] && command -v clang >/dev/null 2>&1; then
            case " ${RUSTFLAGS:-} " in
                *" -fuse-ld="*) ;;
                *)
                    export RUSTFLAGS="${RUSTFLAGS:+${RUSTFLAGS} }-C linker=clang -C link-arg=-fuse-ld=${lld_bin}"
                    echo "ℹ️  Rust 链接加速: ${lld_bin}"
                    ;;
            esac
        fi
    fi
}

# 本机编译加速：增量、外置盘 target 迁移、sccache（不交叉编译、不碰 zigbuild）
apply_dev_rust_compile_accel() {
    ensure_dev_brew_tooling

    # Cursor/沙箱可能注入临时 CARGO_TARGET_DIR；对本地开发应忽略，避免编到 /var/folders/.../cursor-sandbox-cache
    case "${CARGO_TARGET_DIR:-}" in
        *cursor-sandbox-cache*|*Cursor-sandbox*|"")
            unset CARGO_TARGET_DIR
            ;;
    esac

    # 仓库在 /Volumes（外置/网络盘）时，把 target 放到本机 SSD 缓存，避免 IO 拖慢冷/热编译
    # 可用 TOKENSBYTE_LOCAL_TARGET=0 关闭；=1 强制开启
    local want_local_target=0
    case "${TOKENSBYTE_LOCAL_TARGET:-0}" in
        0|false|no|off) want_local_target=0 ;;
        1|true|yes|on) want_local_target=1 ;;
        *)
            case "${ROOT_DIR}" in
                /Volumes/*) want_local_target=1 ;;
            esac
            ;;
    esac
    if [ "${want_local_target}" -eq 1 ] && [ -z "${CARGO_TARGET_DIR:-}" ]; then
        local cache_root="${HOME}/Library/Caches/tokensbyte-dev/target"
        mkdir -p "${cache_root}"
        export CARGO_TARGET_DIR="${cache_root}/${STATE_ID}"
        mkdir -p "${CARGO_TARGET_DIR}"
        echo "ℹ️  检测到外置盘路径，CARGO_TARGET_DIR -> ${CARGO_TARGET_DIR}"
        echo "   （勿删此目录；删掉会触发整包冷编译）"
    elif [ -n "${CARGO_TARGET_DIR:-}" ]; then
        mkdir -p "${CARGO_TARGET_DIR}"
        echo "ℹ️  使用已设置的 CARGO_TARGET_DIR=${CARGO_TARGET_DIR}"
    fi

    if [ "${TOKENSBYTE_SCCACHE:-1}" = "0" ]; then
        echo "ℹ️  已关闭 sccache (TOKENSBYTE_SCCACHE=0)"
        export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-1}"
    elif [ -n "${RUSTC_WRAPPER:-}" ]; then
        echo "ℹ️  使用已设置的 RUSTC_WRAPPER=${RUSTC_WRAPPER}"
        # sccache 禁止增量编译；非 sccache wrapper 则保留增量
        case "${RUSTC_WRAPPER}" in
            *sccache*)
                unset CARGO_INCREMENTAL
                echo "ℹ️  sccache 已启用，已关闭 CARGO_INCREMENTAL（二者不兼容）"
                ;;
            *)
                export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-1}"
                ;;
        esac
    elif command -v sccache >/dev/null 2>&1 && [ "${TOKENSBYTE_SCCACHE:-0}" = "1" ]; then
        export RUSTC_WRAPPER="$(command -v sccache)"
        unset CARGO_INCREMENTAL
        echo "ℹ️  Rust 编译缓存: sccache（已关闭 CARGO_INCREMENTAL，与 sccache 不兼容）"
    else
        unset RUSTC_WRAPPER
        export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-1}"
        echo "ℹ️  Rust 增量加速: Cargo 原生 incremental"
    fi


    apply_dev_rust_link_accel
}

# 回收本仓库残留前后端（不影响其它目录实例 / 共享 Postgres）
reclaim_repo_services() {
    local pid cmd
    echo "🧹 清理本仓库残留的前后端占用..."

    if [ -f "${STATE_FILE}" ]; then
        kill_tree "$(state_get BACKEND_PID)"
        kill_tree "$(state_get FRONTEND_PID)"
        free_listen_port "$(state_get BACKEND_PORT)"
        free_listen_port "$(state_get FRONTEND_PORT)"
        rm -f "${STATE_FILE}"
    fi

    while read -r pid cmd; do
        [ -n "${pid}" ] && [ "${pid}" != "$$" ] || continue
        case "${cmd}" in
            *"${ROOT_DIR}/frontend"*|*"${ROOT_DIR}/backend"*) hard_kill_pid "${pid}" ;;
            *tokensbyte-server*|*cargo-watch*|*"cargo run"*)
                local pcwd
                pcwd="$(proc_cwd "${pid}")"
                case "${pcwd}" in
                    *"${ROOT_DIR}"*) hard_kill_pid "${pid}" ;;
                esac
                ;;
        esac
    done <<EOF
$(ps -axo pid= -o command= 2>/dev/null || true)
EOF

}

docker_pg_ready() {
    local cname="$1"
    [ -n "${cname}" ] || return 1
    docker exec "${cname}" pg_isready -U "${POSTGRES_USER}" >/dev/null 2>&1
}

# 多套开发环境共用同一 Postgres（优先已在跑的实例）
shared_pg_ready() {
    # 端口已监听即视为可用（本机 Postgres / 外部库 / 已起的容器）
    port_in_use "${POSTGRES_PORT}" && return 0
    if command -v pg_isready >/dev/null 2>&1; then
        pg_isready -h 127.0.0.1 -p "${POSTGRES_PORT}" -U "${POSTGRES_USER}" >/dev/null 2>&1 && return 0
    fi
    command -v docker >/dev/null 2>&1 || return 1
    local cname
    for cname in "tokensbyte-ws-postgres" "${PROJECT_NAME}-postgres" "tokensbyte-postgres"; do
        docker_pg_ready "${cname}" && return 0
    done
    docker_pg_ready "$(docker ps --filter "publish=${POSTGRES_PORT}" --format '{{.ID}}' 2>/dev/null | head -n1)"
}

wait_project_postgres() {
    local i
    for i in $(seq 1 30); do
        if docker_pg_ready "${PROJECT_NAME}-postgres" || port_in_use "${POSTGRES_PORT}"; then
            echo "✅ 数据库已就绪"
            return 0
        fi
        sleep 1
    done
    echo "❌ 数据库启动超时，请检查 Docker"
    exit 1
}

ensure_postgres() {
    export PROJECT_NAME POSTGRES_PORT POSTGRES_USER
    export COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-${PROJECT_NAME}}"

    if shared_pg_ready; then
        echo "✅ 复用本机 Postgres (port ${POSTGRES_PORT})"
        return 0
    fi

    if ! command -v docker >/dev/null 2>&1; then
        echo "❌ 未检测到 Postgres (port ${POSTGRES_PORT})，且未安装 Docker，请先启动本机数据库"
        exit 1
    fi

    echo "🐘 启动 Docker Postgres (${PROJECT_NAME}-postgres)..."
    docker compose -f docker-compose.yml -f docker-compose.dev.yml up -d postgres
    echo "⏳ 等待数据库就绪..."
    wait_project_postgres
}

case "${choice}" in
  2)
    echo "🚀 正在启动 Docker 全容器开发环境..."
    export PROJECT_NAME
    export COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-${PROJECT_NAME}}"
    export BACKEND_PORT="$(pick_free_port "${PREFERRED_BACKEND_PORT}" "后端")"
    export FRONTEND_PORT="$(pick_free_port "${PREFERRED_FRONTEND_PORT}" "前端")"
    export POSTGRES_PORT="$(pick_free_port "${POSTGRES_PORT}" "数据库")"
    echo "   项目: ${PROJECT_NAME}"
    echo "   后端: http://localhost:${BACKEND_PORT}"
    echo "   前端: http://localhost:${FRONTEND_PORT}"
    echo "   数据库: localhost:${POSTGRES_PORT}"
    echo "   按 Ctrl+C 停止所有服务"
    docker compose -f docker-compose.yml -f docker-compose.dev.yml up --build
    ;;

  1|"")
    if [ "${DEV_ATTACH}" = "1" ]; then
        echo "🚀 正在前台启动本地开发环境（日志输出到本终端）..."
    else
        echo "🚀 正在后台启动本地开发环境..."
    fi
    echo "   项目: ${PROJECT_NAME}"

    reclaim_repo_services
    ensure_postgres

    if ! command -v cargo-watch >/dev/null 2>&1; then
        echo "⚠️ 未找到 cargo-watch，正在尝试自动安装..."
        cargo install cargo-watch
    fi

    if [ ! -d "frontend/node_modules" ]; then
        echo "📦 正在安装前端依赖 (使用国内镜像源)..."
        (cd frontend && npm install --registry=https://registry.npmmirror.com)
    fi

    # 固定默认端口：先关掉占用 3000/5173 的进程，再原端口启动（不顺延改端口）
    BACKEND_PORT="${PREFERRED_BACKEND_PORT}"
    FRONTEND_PORT="${PREFERRED_FRONTEND_PORT}"
    echo "🧹 释放默认端口 :${BACKEND_PORT} / :${FRONTEND_PORT} ..."
    free_listen_port "${BACKEND_PORT}"
    free_listen_port "${FRONTEND_PORT}"
    export BACKEND_PORT FRONTEND_PORT
    export PORT="${BACKEND_PORT}"
    export HOST="${HOST:-0.0.0.0}"
    DATABASE_URL="${DATABASE_URL:-postgres://tokensapi:tokensapi@127.0.0.1:${POSTGRES_PORT}/tokensapi}"
    case "${DATABASE_URL}" in
        *@postgres:*)
            DATABASE_URL=$(echo "${DATABASE_URL}" | sed 's/@postgres:/@127.0.0.1:/')
            ;;
    esac
    export DATABASE_URL
    export RUST_LOG="${RUST_LOG:-info}"
    export BASE_URL="${BASE_URL:-http://localhost:${BACKEND_PORT}}"
    export VITE_API_TARGET="http://127.0.0.1:${BACKEND_PORT}"
    apply_dev_rust_compile_accel

    echo "⚙️ 启动 Rust 服务 (watch, :${BACKEND_PORT})..."
    : > backend_dev.log
    export PORT HOST DATABASE_URL RUST_LOG BASE_URL
    unset CARGO_INCREMENTAL
    [ -n "${CARGO_TARGET_DIR:-}" ] && export CARGO_TARGET_DIR
    [ -n "${RUSTFLAGS:-}" ] && export RUSTFLAGS
    [ -n "${RUSTC_WRAPPER:-}" ] && export RUSTC_WRAPPER
    # shellcheck disable=SC2086
    if [ -n "${DEV_CARGO_WRAPPER}" ]; then
        daemonize_run "${ROOT_DIR}/backend" "${ROOT_DIR}/backend_dev.log" -- \
            sh -c "exec ${DEV_CARGO_WRAPPER} cargo watch -w src -w Cargo.toml -w Cargo.lock -w build.rs -x run"
    else
        daemonize_run "${ROOT_DIR}/backend" "${ROOT_DIR}/backend_dev.log" -- \
            sh -c "exec cargo watch -w src -w Cargo.toml -w Cargo.lock -w build.rs -x run"
    fi


    BACKEND_PID="$(lsof -nP -tiTCP:"${BACKEND_PORT}" -sTCP:LISTEN 2>/dev/null | head -n1 || true)"

    echo "⚙️ 启动 Vite 服务 (:${FRONTEND_PORT})..."
    : > frontend_dev.log
    export VITE_API_TARGET FRONTEND_PORT
    daemonize_run "${ROOT_DIR}/frontend" "${ROOT_DIR}/frontend_dev.log" -- \
        npm run dev -- --port "${FRONTEND_PORT}" --strictPort --host 0.0.0.0
    # 双 fork 后父进程立刻退出，用端口探测拿真实 PID
    sleep 0.5
    FRONTEND_PID="$(lsof -nP -tiTCP:"${FRONTEND_PORT}" -sTCP:LISTEN 2>/dev/null | head -n1 || true)"
    BACKEND_PID="$(lsof -nP -tiTCP:"${BACKEND_PORT}" -sTCP:LISTEN 2>/dev/null | head -n1 || true)"

    write_run_state

    LOG_FOLLOW_PIDS=""
    stop_log_follow() {
        local p
        for p in ${LOG_FOLLOW_PIDS}; do hard_kill_pid "${p}"; done
        LOG_FOLLOW_PIDS=""
    }

    follow_log() {
        local file="$1" prefix="$2"
        (
            tail -n 0 -F "${file}" 2>/dev/null | while IFS= read -r line; do
                printf '[%s] %s\n' "${prefix}" "${line}"
            done
        ) &
        LOG_FOLLOW_PIDS="${LOG_FOLLOW_PIDS} $!"
    }

    cleanup_attach() {
        stop_log_follow
        echo ""
        echo "🛑 正在停止本实例服务..."
        kill_tree "${BACKEND_PID}"
        kill_tree "${FRONTEND_PID}"
        free_listen_port "${BACKEND_PORT}"
        free_listen_port "${FRONTEND_PORT}"
        rm -f "${STATE_FILE}"
        echo "✅ 本实例已停止，端口已释放"
        exit 0
    }

    if [ "${DEV_ATTACH}" = "1" ]; then
        trap cleanup_attach INT TERM
        echo "📺 前台日志模式（Ctrl+C 停止本实例）"
        follow_log backend_dev.log Rust
        follow_log frontend_dev.log Vite
    fi

    WAIT_MAX=${DEV_WAIT_MAX:-600}
    echo "⏳ 等待后端 (${BACKEND_PORT}) 和前端 (${FRONTEND_PORT}) 就绪（最长 ${WAIT_MAX}s）..."
    backend_up=0
    frontend_up=0
    for i in $(seq 1 "${WAIT_MAX}"); do
        if [ "${backend_up}" -eq 0 ] && port_in_use "${BACKEND_PORT}"; then
            backend_up=1
            echo "✅ 后端已监听 :${BACKEND_PORT}"
        fi
        if [ "${frontend_up}" -eq 0 ] && port_in_use "${FRONTEND_PORT}"; then
            frontend_up=1
            echo "✅ 前端已监听 :${FRONTEND_PORT}"
        fi
        if [ "${backend_up}" -eq 1 ] && [ "${frontend_up}" -eq 1 ]; then
            echo "🎉 本地开发环境已就绪！"
            echo "   👉 项目: ${PROJECT_NAME}"
            echo "   👉 前端面板: http://localhost:${FRONTEND_PORT}"
            echo "   👉 后端 API: http://localhost:${BACKEND_PORT}"
            echo "   👉 数据库: localhost:${POSTGRES_PORT} (共享可复用)"
            echo "   (日志文件: backend_dev.log / frontend_dev.log)"
            if [ "${DEV_ATTACH}" = "1" ]; then
                echo "📺 持续输出日志中，按 Ctrl+C 停止本实例"
                while kill -0 "${BACKEND_PID}" 2>/dev/null || kill -0 "${FRONTEND_PID}" 2>/dev/null; do
                    sleep 1
                done
                cleanup_attach
            fi
            exit 0
        fi

        if [ "${DEV_ATTACH}" != "1" ] && [ $((i % 15)) -eq 0 ]; then
            tip=""
            if [ "${backend_up}" -eq 0 ]; then
                if kill -0 "${BACKEND_PID}" 2>/dev/null || pgrep -P "${BACKEND_PID}" >/dev/null 2>&1; then
                    tip="后端编译/启动中"
                else
                    tip="后端进程已退出，见 backend_dev.log"
                fi
                last="$(tail -n 1 backend_dev.log 2>/dev/null | tr -d '\r')"
                [ -n "${last}" ] && tip="${tip} | ${last}"
            fi
            [ "${frontend_up}" -eq 0 ] && tip="${tip:+${tip}；}前端未就绪"
            echo "… ${i}s / ${WAIT_MAX}s  ${tip}"
        fi
        sleep 1
    done

    echo "❌ 启动超时，请检查 backend_dev.log / frontend_dev.log"
    echo "   当前: 后端=$([ "${backend_up}" -eq 1 ] && echo 就绪 || echo 未就绪) 前端=$([ "${frontend_up}" -eq 1 ] && echo 就绪 || echo 未就绪)"
    [ "${DEV_ATTACH}" = "1" ] && cleanup_attach
    exit 1
    ;;

  *)
    echo "❌ 无效选项，请使用: ./dev.sh [1|2] [bg|fg]"
    exit 1
    ;;
esac
