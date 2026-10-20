#!/bin/bash
# tkeapi (tokensbyte) opensource
# © 2026 tkeapi.com
# @copyright      Copyright netbcloud/wstianxia
# @license        MIT (https://www.tkeapi.com/)

# TokensByte 零停机热更新脚本（在服务器部署目录内运行，与 docker-compose.yml 同级）
#
# 用法:
#   ./hot-update.sh bootstrap                               首次接入/整体重建：重置为 blue 槽位 + baked 前端，写入状态文件（会有一次短暂中断）
#   ./hot-update.sh backend  [--file x.tar | --image tag]   后端蓝绿热更新（新槽位健康后切流，旧槽位优雅排空后停止）
#   ./hot-update.sh frontend [--file x.tar | --image tag]   前端静态资源热更新（解压新版本，current 符号链接原子切换，不重启容器）
#   ./hot-update.sh all      [--file x.tar | --image tag]   先后端、再前端（API 只增不删，故后端先行）
#   ./hot-update.sh rollback [backend|frontend]             回滚（默认 backend）
#   ./hot-update.sh status                                  查看当前槽位 / 上游 / 前端版本
#
# 前置：离线包已传到部署目录（export-images.sh 产出的 <项目>-backend-*.tar / <项目>-frontend-*.tar）。
# 环境变量：HEALTH_TIMEOUT(秒,默认240) KEEP_RELEASES(默认5) SHUTDOWN_DRAIN_SECS(见 docker-compose.yml)
#
# 约束：
#   1. 迁移必须向后兼容（只加表/列/索引），否则旧槽位在重叠期会读写失败；
#   2. 热更新启用后，不要再手动执行 `docker compose up -d` / `down`；整体重建请用 `bootstrap`；
#   3. 同一时刻只允许一个 hot-update.sh 在运行。

set -euo pipefail
cd "$(dirname "$0")"

_env_val() { grep -E "^$1=" .env 2>/dev/null | tail -1 | cut -d= -f2- | tr -d '"\r' || true; }
PROJECT_NAME="${PROJECT_NAME:-$(_env_val PROJECT_NAME)}"
PROJECT_NAME="${PROJECT_NAME:-tokensbyte}"
export PROJECT_NAME

STATE_FILE=".hot-state"
UPSTREAM_DIR="nginx-upstream"
WEB_DIR="web"
HEALTH_TIMEOUT="${HEALTH_TIMEOUT:-240}"
KEEP_RELEASES="${KEEP_RELEASES:-5}"
BACKEND_IMAGE_BASE="${PROJECT_NAME}-backend"
FRONTEND_IMAGE="${PROJECT_NAME}-frontend:latest"
FRONTEND_CT="${PROJECT_NAME}-frontend"

log() { echo "[hot-update] $*"; }
die() { echo "❌ $*" >&2; exit 1; }

command -v docker >/dev/null 2>&1 || die "未找到 docker"
docker compose version >/dev/null 2>&1 || die "未找到 docker compose v2"
[ -f docker-compose.yml ] || die "当前目录缺少 docker-compose.yml"

# 串行锁，防止并发发布
exec 9>.hot-update.lock
flock -n 9 || die "已有另一个 hot-update.sh 正在运行"

dc() { docker compose --profile green "$@"; }

slot_service() { [ "$1" = "blue" ] && echo "backend" || echo "backend-green"; }
slot_container() { [ "$1" = "blue" ] && echo "${PROJECT_NAME}-backend" || echo "${PROJECT_NAME}-backend-green"; }
slot_image_var() { [ "$1" = "blue" ] && echo "BACKEND_IMAGE_BLUE" || echo "BACKEND_IMAGE_GREEN"; }
other_slot() { [ "$1" = "blue" ] && echo "green" || echo "blue"; }

container_running() { [ "$(docker inspect -f '{{.State.Running}}' "$1" 2>/dev/null || echo false)" = "true" ]; }

load_state() {
    [ -f "$STATE_FILE" ] || die "缺少 $STATE_FILE：请先执行 ./hot-update.sh bootstrap（首次接入需一次整体重建）"
    # shellcheck disable=SC1090
    . "$STATE_FILE"
    : "${ACTIVE_SLOT:?状态文件损坏：缺少 ACTIVE_SLOT}"
}

save_state() {
    {
        echo "ACTIVE_SLOT=${ACTIVE_SLOT}"
        echo "ACTIVE_IMAGE=${ACTIVE_IMAGE:-}"
        echo "PREV_SLOT=${PREV_SLOT:-}"
        echo "PREV_IMAGE=${PREV_IMAGE:-}"
    } > "${STATE_FILE}.tmp"
    mv -f "${STATE_FILE}.tmp" "$STATE_FILE"
}

write_upstream() {
    mkdir -p "$UPSTREAM_DIR"
    printf 'upstream tb_backend { server %s:3000; }\n' "$(slot_service "$1")" > "$UPSTREAM_DIR/active.conf.tmp"
    mv -f "$UPSTREAM_DIR/active.conf.tmp" "$UPSTREAM_DIR/active.conf"
}

reload_nginx() {
    docker exec "$FRONTEND_CT" nginx -t >/dev/null 2>&1 && docker exec "$FRONTEND_CT" nginx -s reload >/dev/null 2>&1
}

wait_healthy() {
    local ct="$1" waited=0
    log "等待 $ct 健康（最长 ${HEALTH_TIMEOUT}s）..."
    while [ "$waited" -lt "$HEALTH_TIMEOUT" ]; do
        container_running "$ct" || { echo "容器 $ct 已退出，最近日志："; docker logs --tail 50 "$ct" 2>&1 || true; return 1; }
        if docker exec "$ct" wget -q -O /dev/null --tries=1 http://127.0.0.1:3000/api/health 2>/dev/null; then
            log "✅ $ct 健康"
            return 0
        fi
        sleep 2; waited=$((waited + 2))
    done
    echo "$ct 健康检查超时，最近日志："; docker logs --tail 50 "$ct" 2>&1 || true
    return 1
}

# 等待新槽位接管单例后台任务（旧槽位退出后释放 leader 锁）
wait_leader() {
    local ct="$1" waited=0
    while [ "$waited" -lt 90 ]; do
        if docker logs "$ct" 2>&1 | grep -q "已获得 leader 锁"; then
            log "✅ $ct 已接管单例后台任务"
            return 0
        fi
        sleep 3; waited=$((waited + 3))
    done
    log "⚠️  90s 内未在 $ct 日志中看到 leader 接管，请检查旧槽位是否已退出：docker logs $ct | grep WorkersLeader"
}

# 启动 $1 槽位（镜像 $2），健康后切流，再优雅停止另一个槽位
activate_slot() {
    local target="$1" image="$2" old svc old_svc ct old_ct prev_conf
    old="$(other_slot "$target")"
    svc="$(slot_service "$target")"; old_svc="$(slot_service "$old")"
    ct="$(slot_container "$target")"; old_ct="$(slot_container "$old")"

    log "启动 $target 槽位 ($svc)，镜像 $image"
    dc rm -sf "$svc" >/dev/null 2>&1 || true
    env "$(slot_image_var "$target")=$image" docker compose --profile green up -d --no-deps "$svc"

    if ! wait_healthy "$ct"; then
        dc rm -sf "$svc" >/dev/null 2>&1 || true
        die "新槽位未通过健康检查，已清理，线上流量不受影响"
    fi

    prev_conf="$(cat "$UPSTREAM_DIR/active.conf" 2>/dev/null || true)"
    write_upstream "$target"
    if ! reload_nginx; then
        [ -n "$prev_conf" ] && printf '%s\n' "$prev_conf" > "$UPSTREAM_DIR/active.conf"
        dc rm -sf "$svc" >/dev/null 2>&1 || true
        die "nginx 配置校验/重载失败，已恢复上游并清理新槽位，线上流量不受影响"
    fi
    log "✅ 流量已切到 $target"

    # nginx 旧 worker 会处理完在途请求后退出；稍等片刻再通知旧槽位优雅停机
    sleep 3
    if container_running "$old_ct"; then
        log "优雅停止旧槽位 $old ($old_svc)，排空在途请求..."
        dc stop "$old_svc" || log "⚠️  旧槽位停止返回非 0，请手动确认：docker ps"
    fi
    wait_leader "$ct"
}

newest_tar() { ls -1t "$1"-*.tar 2>/dev/null | head -1 || true; }

parse_file_arg() {
    TAR_FILE=""
    IMAGE_NAME=""
    while [ $# -gt 0 ]; do
        case "$1" in
            --file) TAR_FILE="${2:-}"; shift 2 ;;
            --image) IMAGE_NAME="${2:-}"; shift 2 ;;
            *) shift ;;
        esac
    done
}

cmd_bootstrap() {
    log "bootstrap：整体重建为 blue 槽位（会有一次短暂中断），请确认镜像已导入（import-images.sh）"
    mkdir -p "$WEB_DIR/releases" "$UPSTREAM_DIR"
    printf 'upstream tb_backend { server %s:3000; }\n' "backend" > "$UPSTREAM_DIR/active.conf"
    dc rm -sf backend-green >/dev/null 2>&1 || true
    docker compose up -d
    wait_healthy "$(slot_container blue)" || die "blue 槽位未健康，请检查日志"
    # 前端容器启动入口会把镜像内置 dist 播种为 releases/baked；整体重建以镜像为准，因此 current 指回 baked
    local waited=0
    while [ ! -d "$WEB_DIR/releases/baked" ] && [ "$waited" -lt 30 ]; do sleep 1; waited=$((waited + 1)); done
    [ -d "$WEB_DIR/releases/baked" ] || die "未找到 $WEB_DIR/releases/baked，frontend 容器是否启动成功？"
    ln -sfn releases/baked "$WEB_DIR/current.tmp" && mv -Tf "$WEB_DIR/current.tmp" "$WEB_DIR/current"
    ACTIVE_SLOT=blue; ACTIVE_IMAGE="$(docker inspect -f '{{.Config.Image}}' "$(slot_container blue)")"; PREV_SLOT=""; PREV_IMAGE=""
    save_state
    log "✅ bootstrap 完成，后续可使用 ./hot-update.sh backend|frontend|all 零停机发布"
}

cmd_backend() {
    parse_file_arg "$@"
    load_state
    local active_ct target tarfile ts prev_id new_image new_id
    active_ct="$(slot_container "$ACTIVE_SLOT")"
    container_running "$active_ct" || die "当前活跃槽位 $ACTIVE_SLOT ($active_ct) 未运行，请先排查或执行 bootstrap"

    ts="$(date +%Y%m%d%H%M%S)"
    target="$(other_slot "$ACTIVE_SLOT")"

    # 先给当前运行镜像打回滚 tag，避免覆盖后失去引用
    prev_id="$(docker inspect -f '{{.Image}}' "$active_ct")"
    docker tag "$prev_id" "${BACKEND_IMAGE_BASE}:rollback-${ts}"

    if [ -n "$IMAGE_NAME" ]; then
        log "使用在线镜像 $IMAGE_NAME ..."
        docker pull "$IMAGE_NAME" || true
        new_image="$IMAGE_NAME"
    else
        tarfile="${TAR_FILE:-$(newest_tar "${PROJECT_NAME}-backend")}"
        [ -n "$tarfile" ] && [ -f "$tarfile" ] || die "未找到后端离线包 ${PROJECT_NAME}-backend-*.tar（可用 --file 或 --image 指定）"
        log "加载 $tarfile ..."
        docker load -i "$tarfile"
        new_image="${BACKEND_IMAGE_BASE}:hot-${ts}"
        docker tag "${BACKEND_IMAGE_BASE}:latest" "$new_image"
    fi

    new_id="$(docker inspect -f '{{.Id}}' "$new_image")"
    [ "$new_id" != "$prev_id" ] || die "目标后端镜像与当前运行版本相同，无需更新"

    activate_slot "$target" "$new_image"

    PREV_SLOT="$ACTIVE_SLOT"; PREV_IMAGE="${BACKEND_IMAGE_BASE}:rollback-${ts}"
    ACTIVE_SLOT="$target"; ACTIVE_IMAGE="$new_image"
    save_state
    log "✅ 后端热更新完成：活跃槽位 $ACTIVE_SLOT，镜像 $ACTIVE_IMAGE（旧槽位保留为已停止容器，可 rollback）"
}

cmd_backend_rollback() {
    load_state
    [ -n "${PREV_SLOT:-}" ] && [ -n "${PREV_IMAGE:-}" ] || die "没有可回滚的上一版本"
    local cur_slot="$ACTIVE_SLOT" cur_image="${ACTIVE_IMAGE:-}"
    log "回滚后端：$cur_slot -> $PREV_SLOT ($PREV_IMAGE)；迁移向后兼容，旧版可直接读写已迁移的库"
    activate_slot "$PREV_SLOT" "$PREV_IMAGE"
    ACTIVE_SLOT="$PREV_SLOT"; ACTIVE_IMAGE="$PREV_IMAGE"; PREV_SLOT="$cur_slot"; PREV_IMAGE="$cur_image"
    save_state
    log "✅ 后端已回滚到槽位 $ACTIVE_SLOT"
}

current_release() { basename "$(readlink "$WEB_DIR/current" 2>/dev/null || true)"; }

cmd_frontend() {
    parse_file_arg "$@"
    [ -L "$WEB_DIR/current" ] || die "未找到 $WEB_DIR/current，请先执行 bootstrap"
    local tarfile ts rel cid d
    ts="$(date +%Y%m%d%H%M%S)"
    rel="$WEB_DIR/releases/$ts"

    if [ -n "$IMAGE_NAME" ]; then
        log "从在线前端镜像 $IMAGE_NAME 提取静态资源 ..."
        docker pull "$IMAGE_NAME" || true
        FRONTEND_IMAGE="$IMAGE_NAME"
    else
        tarfile="${TAR_FILE:-$(newest_tar "${PROJECT_NAME}-frontend")}"
        [ -n "$tarfile" ] && [ -f "$tarfile" ] || die "未找到前端离线包 ${PROJECT_NAME}-frontend-*.tar（可用 --file 或 --image 指定）"
        log "加载 $tarfile 并提取静态资源 ..."
        docker load -i "$tarfile"
    fi
    mkdir -p "$rel"
    cid="$(docker create "$FRONTEND_IMAGE")"
    docker cp "$cid:/srv/web-baked/." "$rel/" || { docker rm -f "$cid" >/dev/null; rm -rf "$rel"; die "提取静态资源失败"; }
    docker rm -f "$cid" >/dev/null
    [ -f "$rel/index.html" ] || { rm -rf "$rel"; die "提取结果缺少 index.html"; }

    # 合入最近 2 个旧版本的 assets（不覆盖），避免打开着旧页面的用户请求旧 chunk 时 404
    for d in $(ls -1 "$WEB_DIR/releases" | grep -E '^[0-9]{14}$' | grep -v "^${ts}$" | sort -r | head -2); do
        if [ -d "$WEB_DIR/releases/$d/assets" ]; then
            mkdir -p "$rel/assets"
            cp -rn "$WEB_DIR/releases/$d/assets/." "$rel/assets/" 2>/dev/null || true
        fi
    done

    ln -sfn "releases/$ts" "$WEB_DIR/current.tmp" && mv -Tf "$WEB_DIR/current.tmp" "$WEB_DIR/current"
    log "✅ 前端已切换到 releases/$ts（无需重启容器）"

    # 仅保留最近 KEEP_RELEASES 个版本（baked 与当前版本永不删除）
    local keep_cur; keep_cur="$(current_release)"
    for d in $(ls -1 "$WEB_DIR/releases" | grep -E '^[0-9]{14}$' | sort -r | tail -n +$((KEEP_RELEASES + 1))); do
        [ "$d" = "$keep_cur" ] || rm -rf "$WEB_DIR/releases/$d"
    done
    log "ℹ️  若本次改动包含 nginx 配置（frontend/Dockerfile），还需重建 frontend 容器：docker compose up -d --no-deps frontend（约 1 秒中断）"
}

cmd_frontend_rollback() {
    [ -L "$WEB_DIR/current" ] || die "未找到 $WEB_DIR/current"
    local cur prev
    cur="$(current_release)"
    prev="$(ls -1 "$WEB_DIR/releases" | grep -E '^[0-9]{14}$' | sort -r | grep -v "^${cur}$" | head -1 || true)"
    [ -n "$prev" ] || die "没有可回滚的前端版本"
    ln -sfn "releases/$prev" "$WEB_DIR/current.tmp" && mv -Tf "$WEB_DIR/current.tmp" "$WEB_DIR/current"
    log "✅ 前端已回滚：$cur -> $prev"
}

cmd_status() {
    if [ -f "$STATE_FILE" ]; then echo "--- $STATE_FILE"; cat "$STATE_FILE"; else echo "(无状态文件，尚未 bootstrap)"; fi
    echo "--- upstream"; cat "$UPSTREAM_DIR/active.conf" 2>/dev/null || echo "(无)"
    echo "--- web/current"; readlink "$WEB_DIR/current" 2>/dev/null || echo "(无)"
    echo "--- containers"
    docker ps -a --filter "name=${PROJECT_NAME}-" --format 'table {{.Names}}\t{{.Status}}\t{{.Image}}'
}

main() {
    local cmd="${1:-}"; shift || true
    case "$cmd" in
        bootstrap) cmd_bootstrap ;;
        backend) cmd_backend "$@" ;;
        frontend) cmd_frontend "$@" ;;
        all) cmd_backend "$@"; cmd_frontend "$@" ;;
        rollback)
            case "${1:-backend}" in
                backend) cmd_backend_rollback ;;
                frontend) cmd_frontend_rollback ;;
                *) die "rollback 仅支持 backend|frontend" ;;
            esac ;;
        status) cmd_status ;;
        *) sed -n '7,24p' "$0"; exit 1 ;;
    esac
}

main "$@"
