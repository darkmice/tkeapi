#!/usr/bin/env bash
# 用 Downloads 里的大备份替换本地 tokensapi 库（127.0.0.1:5433）
set -euo pipefail

BACKUP="${1:-$HOME/Downloads/pgsql_backup_20260817_020001 (1).sql.gz}"
LOG="${TMPDIR:-/tmp}/pgsql_restore_manual.log"
export PGPASSWORD="${PGPASSWORD:-tokensapi}"
HOST="${PGHOST:-127.0.0.1}"
PORT="${PGPORT:-5433}"
USER="${PGUSER:-tokensapi}"
DB="${PGDATABASE:-tokensapi}"

if [[ ! -f "$BACKUP" ]]; then
  echo "找不到备份: $BACKUP" >&2
  exit 1
fi

AVAIL_KB=$(df -k /System/Volumes/Data | awk 'NR==2{print $4}')
NEED_KB=$((12 * 1024 * 1024)) # 建议至少约 12GB 空闲
if [[ "$AVAIL_KB" -lt "$NEED_KB" ]]; then
  echo "磁盘空间不足：Data 卷约 $((AVAIL_KB/1024/1024))GB 可用，建议先腾出 ≥12GB 再导入（备份约 1.4GB，解压后约 3.7GB，加上 WAL/膨胀）。" >&2
  df -h /System/Volumes/Data
  exit 1
fi

echo "==> 确认 Postgres 可达 $HOST:$PORT ..."
psql -h "$HOST" -p "$PORT" -U "$USER" -d "$DB" -c 'SELECT 1' >/dev/null

echo "==> 清空 public schema"
psql -h "$HOST" -p "$PORT" -U "$USER" -d "$DB" -v ON_ERROR_STOP=1 -c \
  "DROP SCHEMA public CASCADE; CREATE SCHEMA public; GRANT ALL ON SCHEMA public TO $USER; GRANT ALL ON SCHEMA public TO public;"

echo "==> 开始还原: $BACKUP"
echo "    日志: $LOG"
gunzip -c "$BACKUP" | psql -h "$HOST" -p "$PORT" -U "$USER" -d "$DB" -v ON_ERROR_STOP=1 >"$LOG" 2>&1

echo "==> 完成，抽查："
psql -h "$HOST" -p "$PORT" -U "$USER" -d "$DB" -c \
  "SELECT count(*) AS tables FROM information_schema.tables WHERE table_schema='public';"
psql -h "$HOST" -p "$PORT" -U "$USER" -d "$DB" -c "SELECT count(*) AS users FROM users;"
psql -h "$HOST" -p "$PORT" -U "$USER" -d "$DB" -c "SELECT count(*) AS models FROM models;"
echo "DONE"
