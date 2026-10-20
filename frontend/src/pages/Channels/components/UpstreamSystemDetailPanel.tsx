/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useMemo, useState } from 'react';
import { Link } from 'react-router-dom';
import {
  Button,
  Card,
  Empty,
  Popover,
  Segmented,
  Space,
  Spin,
  Switch,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import {
  ApiOutlined,
  CheckCircleOutlined,
  ClockCircleOutlined,
  CloseCircleOutlined,
  DownOutlined,
  EditOutlined,
  LineChartOutlined,
  LinkOutlined,
  ReloadOutlined,
  RightOutlined,
  SwapRightOutlined,
  SyncOutlined,
  ThunderboltOutlined,
} from '@ant-design/icons';
import {
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip as RechartsTooltip,
  XAxis,
  YAxis,
} from 'recharts';
import dayjs from 'dayjs';
import ErrorBoundary from '../../../components/ErrorBoundary';
import type { ChannelCategory, ChannelConfig } from '../../../types';
import { formatApiDateTime } from '../../../utils/timedisplay';
import { channelEditPath } from '../channelPaths';

const { Text } = Typography;

export interface ChannelConfigSyncLog {
  id: number;
  config_id: number;
  upstream_system: string;
  upstream_group: string;
  upstream_ratio: number;
  rate_add: number;
  upstream_currency: string;
  site_currency: string;
  currency_rate: number;
  previous_rate: number;
  applied_rate: number;
  sync_interval_minutes: number;
  trigger_type: string;
  status: number;
  error_message: string;
  created_at: string;
}

interface UpstreamSystemDetailPanelProps {
  systemKey: string;
  configs: ChannelConfig[];
  categories: ChannelCategory[];
  isLight: boolean;
  adminPath: string;
  syncLogsMap: Record<number, ChannelConfigSyncLog[]>;
  syncLogsLoadingMap: Record<number, boolean>;
  syncTriggerLoadingMap: Record<number, boolean>;
  onRefreshSystemLogs: (targetConfigs: ChannelConfig[]) => void;
  onManualSyncRate: (record: ChannelConfig) => Promise<void>;
  onEditConfig: (record: ChannelConfig) => void;
  onClearSystemFilter: () => void;
}

const LINE_COLORS = [
  '#1677ff',
  '#10b981',
  '#f59e0b',
  '#8b5cf6',
  '#ec4899',
  '#06b6d4',
  '#ef4444',
  '#84cc16',
];

const SYSTEM_DESCRIPTIONS: Record<string, string> = {
  newapi: '支持拉取上游 /api/pricing 分组倍率、跨币种汇率换算、叠加增量与定时自动同步渠道倍率',
  Tkeapi: 'TokensByte (Tkeapi) 标准上游系统接入，支持多模型渠道分组高可用调度与额度管控',
  官方: '官方原生 API 端点直连，提供稳定标准协议鉴权与多维度配额保护',
  兼容: 'OpenAI 兼容格式通用上游端点接入，灵活适配第三方聚合与自建推理服务',
  火山引擎: '火山方舟 / 豆包 / 即梦视频等字节跳动云原生大模型与多媒体任务端点',
  阿里云: '阿里云百炼 (DashScope) 大模型与通义万相多模态服务接入端点',
  __unclassified__: '尚未指定具体上游系统分类的渠道预设配置',
};

function resolveCategoryIds(record: { category_ids?: number[]; category_id?: number | null }): number[] {
  if (Array.isArray(record.category_ids) && record.category_ids.length > 0) {
    return record.category_ids;
  }
  if (record.category_id != null && record.category_id > 0) {
    return [record.category_id];
  }
  return [];
}

function formatQuotaSummary(used?: number, limit?: number): string {
  const l = Number(limit ?? -1);
  const u = Number(used ?? 0);
  const fmt = (n: number) => (Number.isInteger(n) ? String(n) : n.toFixed(4).replace(/\.?0+$/, ''));
  if (l < 0) return `已用 ${fmt(u)} / ∞`;
  return `${fmt(u)} / ${fmt(l)}`;
}

const UpstreamSystemDetailPanel: React.FC<UpstreamSystemDetailPanelProps> = ({
  systemKey,
  configs,
  categories,
  isLight,
  adminPath,
  syncLogsMap,
  syncLogsLoadingMap,
  syncTriggerLoadingMap,
  onRefreshSystemLogs,
  onManualSyncRate,
  onEditConfig,
  onClearSystemFilter,
}) => {
  const [collapsed, setCollapsed] = useState(false);
  const [selectedConfigId, setSelectedConfigId] = useState<number | 'all'>('all');
  const [chartMetric, setChartMetric] = useState<'applied_rate' | 'upstream_ratio'>('applied_rate');
  const [onlyRateChanged, setOnlyRateChanged] = useState(false);
  const [historyFilter, setHistoryFilter] = useState<'all' | 'changed' | 'error'>('all');
  const [batchSyncing, setBatchSyncing] = useState(false);

  const systemLabel = systemKey === '__unclassified__' ? '未分类系统' : systemKey;
  const systemDesc = SYSTEM_DESCRIPTIONS[systemKey] || `绑定「${systemLabel}」的上游渠道配置详情与分组倍率变动监控`;

  const effectiveSelectedId = useMemo(() => {
    if (selectedConfigId === 'all') return 'all';
    return configs.some(c => c.id === selectedConfigId) ? selectedConfigId : 'all';
  }, [selectedConfigId, configs]);

  const resolveCategoryName = (id: number) => categories.find(c => c.id === id)?.name || '';

  // 汇总统计指标
  const summary = useMemo(() => {
    const total = configs.length;
    const active = configs.filter(c => (c.status ?? 1) === 1).length;
    const disabled = total - active;

    const uniqueBoundGroupIds = new Set<number>();
    let haGroupBindings = 0;
    let directBindings = 0;
    configs.forEach(c => {
      (c.bound_channels || []).forEach(ch => {
        uniqueBoundGroupIds.add(ch.id);
        if (ch.is_ha) haGroupBindings += 1;
        else directBindings += 1;
      });
    });

    const rates = configs.map(c => Number(c.rate ?? 1)).filter(n => Number.isFinite(n));
    const minRate = rates.length ? Math.min(...rates) : 0;
    const maxRate = rates.length ? Math.max(...rates) : 0;

    const syncedGroupConfigs = configs.filter(c => Boolean((c.upstream_group || '').trim()));
    const autoSyncConfigs = configs.filter(
      c => Boolean((c.upstream_group || '').trim()) && Number(c.upstream_sync_interval_minutes) > 0,
    );

    const uniqueUpstreamGroups = Array.from(
      new Set(configs.map(c => (c.upstream_group || '').trim()).filter(Boolean)),
    );

    let totalLogs = 0;
    let changedLogsCount = 0;
    configs.forEach(c => {
      const logs = syncLogsMap[c.id] || [];
      totalLogs += logs.length;
      logs.forEach(l => {
        if (l.status === 1 && Math.abs((l.previous_rate ?? 0) - (l.applied_rate ?? 0)) > 0.00001) {
          changedLogsCount += 1;
        }
      });
    });

    return {
      total,
      active,
      disabled,
      uniqueBoundGroups: uniqueBoundGroupIds.size,
      haGroupBindings,
      directBindings,
      minRate,
      maxRate,
      syncedGroupCount: syncedGroupConfigs.length,
      autoSyncCount: autoSyncConfigs.length,
      uniqueUpstreamGroups,
      totalLogs,
      changedLogsCount,
    };
  }, [configs, syncLogsMap]);

  const anyLoadingLogs = useMemo(
    () => configs.some(c => Boolean(syncLogsLoadingMap[c.id])),
    [configs, syncLogsLoadingMap],
  );

  // 合并所有历史记录（带渠道信息）
  const mergedHistoryLogs = useMemo(() => {
    const configMap = new Map<number, ChannelConfig>();
    configs.forEach(c => configMap.set(c.id, c));

    const list: (ChannelConfigSyncLog & {
      configName: string;
      configYid: string;
      rateChanged: boolean;
      rateDelta: number;
    })[] = [];

    configs.forEach(c => {
      if (effectiveSelectedId !== 'all' && c.id !== effectiveSelectedId) return;
      const logs = syncLogsMap[c.id] || [];
      logs.forEach(log => {
        const prev = Number(log.previous_rate ?? 0);
        const applied = Number(log.applied_rate ?? 0);
        const delta = Number((applied - prev).toFixed(4));
        const rateChanged = log.status === 1 && Math.abs(delta) > 0.00001;
        list.push({
          ...log,
          configName: c.name,
          configYid: c.yid || '-',
          rateChanged,
          rateDelta: delta,
        });
      });
    });

    list.sort((a, b) => {
      const ta = a.created_at || '';
      const tb = b.created_at || '';
      return tb.localeCompare(ta) || b.id - a.id;
    });

    return list;
  }, [configs, syncLogsMap, effectiveSelectedId]);

  const filteredHistoryLogs = useMemo(() => {
    return mergedHistoryLogs.filter(item => {
      if (historyFilter === 'changed') return item.rateChanged;
      if (historyFilter === 'error') return item.status !== 1;
      return true;
    });
  }, [mergedHistoryLogs, historyFilter]);

  // 构建折线图数据
  const chartDataInfo = useMemo(() => {
    const targetConfigs =
      effectiveSelectedId === 'all'
        ? configs
        : configs.filter(c => c.id === effectiveSelectedId);

    // 单渠道模式：展示「本站生效渠道倍率 (applied_rate)」、「上游分组倍率 (upstream_ratio)」、「变动前原倍率 (previous_rate)」
    if (effectiveSelectedId !== 'all' && targetConfigs.length === 1) {
      const cfg = targetConfigs[0];
      const rawLogs = (syncLogsMap[cfg.id] || []).filter(l => l.status === 1);
      const filtered = onlyRateChanged
        ? rawLogs.filter(l => Math.abs((l.previous_rate ?? 0) - (l.applied_rate ?? 0)) > 0.00001)
        : rawLogs;

      const chronological = [...filtered].reverse();

      // 若暂无历史同步记录，使用当前渠道倍率生成基准点以便直观查看
      const points =
        chronological.length > 0
          ? chronological.map(l => {
              const delta = Number(((l.applied_rate ?? 0) - (l.previous_rate ?? 0)).toFixed(4));
              return {
                timeLabel: l.created_at ? dayjs(l.created_at).format('MM-DD HH:mm') : '-',
                fullTime: l.created_at ? formatApiDateTime(l.created_at) : '-',
                applied_rate: Number((l.applied_rate ?? 0).toFixed(4)),
                upstream_ratio: Number((l.upstream_ratio ?? 0).toFixed(4)),
                previous_rate: Number((l.previous_rate ?? 0).toFixed(4)),
                rate_add: Number(l.rate_add ?? 0),
                upstream_group: l.upstream_group || cfg.upstream_group || '-',
                upstream_currency: l.upstream_currency || cfg.upstream_currency || '',
                site_currency: l.site_currency || cfg.site_currency || '',
                currency_rate: l.currency_rate || cfg.upstream_currency_rate || 1,
                trigger_type: l.trigger_type,
                delta,
                configName: cfg.name,
                configYid: cfg.yid || '-',
                isBaseline: false,
              };
            })
          : [
              {
                timeLabel: '当前生效',
                fullTime: cfg.updated_at || cfg.created_at ? formatApiDateTime(cfg.updated_at || cfg.created_at) : '当前状态',
                applied_rate: Number((cfg.rate ?? 1).toFixed(4)),
                upstream_ratio: Number((cfg.rate ?? 1).toFixed(4)),
                previous_rate: Number((cfg.rate ?? 1).toFixed(4)),
                rate_add: Number(cfg.upstream_sync_rate_add ?? 0),
                upstream_group: cfg.upstream_group || '未配置分组',
                upstream_currency: cfg.upstream_currency || '',
                site_currency: cfg.site_currency || '',
                currency_rate: cfg.upstream_currency_rate || 1,
                trigger_type: 'baseline',
                delta: 0,
                configName: cfg.name,
                configYid: cfg.yid || '-',
                isBaseline: true,
              },
            ];

      return {
        mode: 'single' as const,
        points,
        hasRealLogs: chronological.length > 0,
        series: [] as { key: string; name: string; color: string; configId: number }[],
      };
    }

    // 多渠道对比模式：按时间轴聚合各渠道的倍率点
    const series = targetConfigs.map((c, idx) => ({
      key: `cfg_${c.id}`,
      name: `${c.name}${c.upstream_group ? ` (${c.upstream_group})` : ''}`,
      color: LINE_COLORS[idx % LINE_COLORS.length],
      configId: c.id,
    }));

    type MultiEvent = {
      timestamp: string;
      timeLabel: string;
      fullTime: string;
      configId: number;
      value: number;
      log?: ChannelConfigSyncLog;
      config: ChannelConfig;
    };

    const events: MultiEvent[] = [];
    let hasAnyRealLog = false;

    targetConfigs.forEach(c => {
      const rawLogs = (syncLogsMap[c.id] || []).filter(l => l.status === 1);
      const filtered = onlyRateChanged
        ? rawLogs.filter(l => Math.abs((l.previous_rate ?? 0) - (l.applied_rate ?? 0)) > 0.00001)
        : rawLogs;

      if (filtered.length > 0) {
        hasAnyRealLog = true;
        filtered.forEach(l => {
          const val = chartMetric === 'applied_rate' ? Number(l.applied_rate ?? c.rate ?? 1) : Number(l.upstream_ratio ?? c.rate ?? 1);
          events.push({
            timestamp: l.created_at || '',
            timeLabel: l.created_at ? dayjs(l.created_at).format('MM-DD HH:mm') : '-',
            fullTime: l.created_at ? formatApiDateTime(l.created_at) : '-',
            configId: c.id,
            value: Number(val.toFixed(4)),
            log: l,
            config: c,
          });
        });
      }
    });

    if (!hasAnyRealLog) {
      // 若所有渠道均尚无同步历史记录，则展示当前各渠道的生效倍率对比点
      const baselinePoints = targetConfigs.map(c => {
        const row: Record<string, any> = {
          timeLabel: c.name,
          fullTime: `${c.name} (${c.yid || '-'}) 当前状态`,
          isBaseline: true,
        };
        row[`cfg_${c.id}`] = Number((c.rate ?? 1).toFixed(4));
        row[`meta_${c.id}`] = {
          configName: c.name,
          configYid: c.yid || '-',
          upstream_group: c.upstream_group || '未配置分组',
          applied_rate: Number((c.rate ?? 1).toFixed(4)),
          upstream_ratio: Number((c.rate ?? 1).toFixed(4)),
          previous_rate: Number((c.rate ?? 1).toFixed(4)),
          rate_add: Number(c.upstream_sync_rate_add ?? 0),
          trigger_type: 'baseline',
        };
        return row;
      });
      return {
        mode: 'multi' as const,
        points: baselinePoints,
        hasRealLogs: false,
        series,
      };
    }

    events.sort((a, b) => a.timestamp.localeCompare(b.timestamp));

    // 按分钟级时间桶聚合并前向填充（Forward-fill）各渠道最近倍率，使多渠道对比折线连续清晰
    const buckets = new Map<string, Record<string, any>>();
    const latestValues: Record<string, number> = {};
    // 初始化各渠道初始值（用各自最早一条记录的 previous_rate 或当前 rate）
    targetConfigs.forEach(c => {
      const logs = (syncLogsMap[c.id] || []).filter(l => l.status === 1);
      if (logs.length > 0) {
        const oldest = logs[logs.length - 1];
        latestValues[`cfg_${c.id}`] =
          chartMetric === 'applied_rate'
            ? Number((oldest.previous_rate ?? oldest.applied_rate ?? c.rate ?? 1).toFixed(4))
            : Number((oldest.upstream_ratio ?? c.rate ?? 1).toFixed(4));
      } else {
        latestValues[`cfg_${c.id}`] = Number((c.rate ?? 1).toFixed(4));
      }
    });

    events.forEach(ev => {
      const bucketKey = ev.timestamp ? dayjs(ev.timestamp).format('YYYY-MM-DD HH:mm:ss') : ev.fullTime;
      latestValues[`cfg_${ev.configId}`] = ev.value;
      const existing = buckets.get(bucketKey) || {
        timeLabel: ev.timeLabel,
        fullTime: ev.fullTime,
        isBaseline: false,
      };
      Object.keys(latestValues).forEach(k => {
        existing[k] = latestValues[k];
      });
      if (ev.log) {
        existing[`meta_${ev.configId}`] = {
          configName: ev.config.name,
          configYid: ev.config.yid || '-',
          upstream_group: ev.log.upstream_group || ev.config.upstream_group || '-',
          applied_rate: Number((ev.log.applied_rate ?? 0).toFixed(4)),
          upstream_ratio: Number((ev.log.upstream_ratio ?? 0).toFixed(4)),
          previous_rate: Number((ev.log.previous_rate ?? 0).toFixed(4)),
          rate_add: Number(ev.log.rate_add ?? 0),
          upstream_currency: ev.log.upstream_currency || '',
          site_currency: ev.log.site_currency || '',
          currency_rate: ev.log.currency_rate || 1,
          trigger_type: ev.log.trigger_type,
        };
      }
      buckets.set(bucketKey, existing);
    });

    return {
      mode: 'multi' as const,
      points: Array.from(buckets.values()),
      hasRealLogs: true,
      series,
    };
  }, [configs, effectiveSelectedId, syncLogsMap, onlyRateChanged, chartMetric]);

  const syncableNewapiConfigs = useMemo(
    () =>
      configs.filter(
        c => c.upstream_system === 'newapi' && Boolean((c.upstream_group || '').trim()),
      ),
    [configs],
  );

  const handleBatchSyncAll = async () => {
    if (syncableNewapiConfigs.length === 0 || batchSyncing) return;
    setBatchSyncing(true);
    try {
      for (const cfg of syncableNewapiConfigs) {
        await onManualSyncRate(cfg);
      }
    } finally {
      setBatchSyncing(false);
    }
  };

  const borderColor = isLight ? 'rgba(0, 0, 0, 0.08)' : 'rgba(255, 255, 255, 0.1)';
  const subtleBg = isLight ? '#f8fafc' : 'rgba(255, 255, 255, 0.03)';
  const cardBg = isLight ? '#ffffff' : '#141414';
  const gridStroke = isLight ? '#e2e8f0' : 'rgba(255, 255, 255, 0.08)';
  const axisColor = isLight ? '#64748b' : '#a1a1aa';

  return (
    <div
      style={{
        marginBottom: 16,
        borderRadius: 10,
        border: isLight ? '1px solid #bae0ff' : '1px solid #15395b',
        backgroundColor: isLight ? '#f0f7ff' : 'rgba(17, 29, 44, 0.65)',
        overflow: 'hidden',
      }}
    >
      {/* 顶部系统概览栏 */}
      <div
        style={{
          padding: '10px 16px',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          flexWrap: 'wrap',
          gap: 10,
          borderBottom: collapsed ? 'none' : (isLight ? '1px solid #d6e8ff' : '1px solid rgba(255,255,255,0.08)'),
        }}
      >
        <Space size={8} wrap style={{ alignItems: 'center' }}>
          <Tag
            color="blue"
            style={{
              margin: 0,
              fontSize: 13,
              fontWeight: 600,
              padding: '2px 10px',
              borderRadius: 6,
            }}
          >
            <ApiOutlined style={{ marginRight: 5 }} />
            上游系统：{systemLabel}
          </Tag>
          <Text type="secondary" style={{ fontSize: 12 }}>
            {systemDesc}
          </Text>
        </Space>

        <Space size={8} wrap>
          <Button
            size="small"
            icon={<ReloadOutlined spin={anyLoadingLogs} />}
            onClick={() => onRefreshSystemLogs(configs)}
          >
            刷新数据
          </Button>
          {syncableNewapiConfigs.length > 0 && (
            <Button
              size="small"
              type="primary"
              ghost
              icon={<SyncOutlined spin={batchSyncing} />}
              loading={batchSyncing}
              onClick={handleBatchSyncAll}
            >
              一键同步全部分组倍率 ({syncableNewapiConfigs.length})
            </Button>
          )}
          <Button
            size="small"
            type="text"
            icon={collapsed ? <RightOutlined /> : <DownOutlined />}
            onClick={() => setCollapsed(!collapsed)}
          >
            {collapsed ? '展开详情与曲线' : '收起面板'}
          </Button>
          <Button size="small" type="link" onClick={onClearSystemFilter} style={{ padding: '0 4px' }}>
            查看全部系统
          </Button>
        </Space>
      </div>

      {!collapsed && (
        <div style={{ padding: '14px 16px', display: 'flex', flexDirection: 'column', gap: 14 }}>
          {/* 1. 核心统计指标卡片行 */}
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fit, minmax(195px, 1fr))',
              gap: 10,
            }}
          >
            <div
              style={{
                background: cardBg,
                border: `1px solid ${borderColor}`,
                borderRadius: 8,
                padding: '10px 12px',
              }}
            >
              <Text type="secondary" style={{ fontSize: 12 }}>绑定该系统的上游渠道</Text>
              <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginTop: 4 }}>
                <span style={{ fontSize: 20, fontWeight: 700 }}>{summary.total} <span style={{ fontSize: 12, fontWeight: 400 }}>个预设</span></span>
                <Space size={4}>
                  <Tag color="success" style={{ margin: 0, fontSize: 11, padding: '0 5px' }}>激活 {summary.active}</Tag>
                  {summary.disabled > 0 && (
                    <Tag color="error" style={{ margin: 0, fontSize: 11, padding: '0 5px' }}>禁用 {summary.disabled}</Tag>
                  )}
                </Space>
              </div>
            </div>

            <div
              style={{
                background: cardBg,
                border: `1px solid ${borderColor}`,
                borderRadius: 8,
                padding: '10px 12px',
              }}
            >
              <Text type="secondary" style={{ fontSize: 12 }}>关联下游模型渠道分组</Text>
              <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginTop: 4 }}>
                <span style={{ fontSize: 20, fontWeight: 700 }}>{summary.uniqueBoundGroups} <span style={{ fontSize: 12, fontWeight: 400 }}>个模型组</span></span>
                <Space size={4}>
                  <Tag color="purple" style={{ margin: 0, fontSize: 11, padding: '0 5px' }}>HA {summary.haGroupBindings}</Tag>
                  <Tag style={{ margin: 0, fontSize: 11, padding: '0 5px' }}>直连 {summary.directBindings}</Tag>
                </Space>
              </div>
            </div>

            <div
              style={{
                background: cardBg,
                border: `1px solid ${borderColor}`,
                borderRadius: 8,
                padding: '10px 12px',
              }}
            >
              <Text type="secondary" style={{ fontSize: 12 }}>当前生效渠道倍率区间</Text>
              <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginTop: 4 }}>
                <span style={{ fontSize: 18, fontWeight: 700, color: '#fa8c16' }}>
                  {summary.total === 0
                    ? '-'
                    : summary.minRate === summary.maxRate
                      ? `${summary.minRate}x`
                      : `${summary.minRate}x ~ ${summary.maxRate}x`}
                </span>
                <Tag color="orange" style={{ margin: 0, fontSize: 11, padding: '0 5px' }}>
                  {summary.uniqueUpstreamGroups.length > 0
                    ? `${summary.uniqueUpstreamGroups.length} 个上游分组`
                    : '标准倍率'}
                </Tag>
              </div>
            </div>

            <div
              style={{
                background: cardBg,
                border: `1px solid ${borderColor}`,
                borderRadius: 8,
                padding: '10px 12px',
              }}
            >
              <Text type="secondary" style={{ fontSize: 12 }}>分组倍率同步与变动统计</Text>
              <div style={{ display: 'flex', alignItems: 'baseline', justifyContent: 'space-between', marginTop: 4 }}>
                <span style={{ fontSize: 18, fontWeight: 700, color: '#1677ff' }}>
                  {summary.changedLogsCount} <span style={{ fontSize: 12, fontWeight: 400 }}>次倍率变动</span>
                </span>
                <Space size={4}>
                  <Tag color="cyan" style={{ margin: 0, fontSize: 11, padding: '0 5px' }}>
                    定时 {summary.autoSyncCount}/{summary.syncedGroupCount}
                  </Tag>
                  <Tag style={{ margin: 0, fontSize: 11, padding: '0 5px' }}>
                    记录 {summary.totalLogs}
                  </Tag>
                </Space>
              </div>
            </div>
          </div>

          {configs.length === 0 ? (
            <Card size="small" style={{ background: cardBg, borderRadius: 8 }}>
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description={`当前暂无绑定「${systemLabel}」的上游渠道配置预设`}
              />
            </Card>
          ) : (
            <>
              {/* 2. 上游渠道分组倍率变动历史曲线图 + 历史记录明细 */}
              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: 'repeat(auto-fit, minmax(340px, 1fr))',
                  gap: 12,
                }}
              >
                {/* 左侧：倍率变动历史曲线图 */}
                <div
                  style={{
                    background: cardBg,
                    border: `1px solid ${borderColor}`,
                    borderRadius: 8,
                    padding: '12px 14px',
                    gridColumn: 'span 1',
                    minWidth: 0,
                  }}
                >
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      flexWrap: 'wrap',
                      gap: 8,
                      marginBottom: 10,
                    }}
                  >
                    <Space size={6} wrap>
                      <LineChartOutlined style={{ color: '#1677ff', fontSize: 15 }} />
                      <span style={{ fontWeight: 600, fontSize: 13 }}>
                        上游渠道分组倍率变动趋势曲线
                      </span>
                      {!chartDataInfo.hasRealLogs && (
                        <Tag color="default" style={{ margin: 0, fontSize: 11 }}>
                          当前基准快照（尚无历史同步点）
                        </Tag>
                      )}
                    </Space>

                    <Space size={8} wrap>
                      {effectiveSelectedId === 'all' && chartDataInfo.hasRealLogs && (
                        <Segmented
                          size="small"
                          value={chartMetric}
                          onChange={(val) => setChartMetric(val as 'applied_rate' | 'upstream_ratio')}
                          options={[
                            { label: '本站生效倍率', value: 'applied_rate' },
                            { label: '上游原始分组倍率', value: 'upstream_ratio' },
                          ]}
                        />
                      )}
                      <Space size={4}>
                        <Switch
                          size="small"
                          checked={onlyRateChanged}
                          onChange={setOnlyRateChanged}
                        />
                        <Text type="secondary" style={{ fontSize: 12 }}>仅看倍率变动点</Text>
                      </Space>
                    </Space>
                  </div>

                  {/* 渠道选择器 */}
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      flexWrap: 'wrap',
                      gap: 6,
                      marginBottom: 10,
                      padding: '6px 8px',
                      background: subtleBg,
                      borderRadius: 6,
                    }}
                  >
                    <Text type="secondary" style={{ fontSize: 12, marginRight: 2 }}>切换渠道曲线：</Text>
                    <Tag
                      color={effectiveSelectedId === 'all' ? 'blue' : undefined}
                      style={{
                        margin: 0,
                        cursor: 'pointer',
                        fontSize: 12,
                        padding: '1px 8px',
                        borderRadius: 4,
                        fontWeight: effectiveSelectedId === 'all' ? 600 : 400,
                      }}
                      onClick={() => setSelectedConfigId('all')}
                    >
                      全部绑定渠道 ({configs.length})
                    </Tag>
                    {configs.map((c, idx) => {
                      const active = effectiveSelectedId === c.id;
                      const logCount = (syncLogsMap[c.id] || []).length;
                      const color = LINE_COLORS[idx % LINE_COLORS.length];
                      return (
                        <Tag
                          key={c.id}
                          color={active ? 'blue' : undefined}
                          style={{
                            margin: 0,
                            cursor: 'pointer',
                            fontSize: 12,
                            padding: '1px 8px',
                            borderRadius: 4,
                            fontWeight: active ? 600 : 400,
                            display: 'inline-flex',
                            alignItems: 'center',
                            gap: 5,
                          }}
                          onClick={() => setSelectedConfigId(c.id)}
                        >
                          <span
                            style={{
                              width: 7,
                              height: 7,
                              borderRadius: '50%',
                              backgroundColor: color,
                              display: 'inline-block',
                            }}
                          />
                          <span>{c.name}</span>
                          {c.upstream_group && (
                            <span style={{ opacity: 0.75, fontSize: 11 }}>[{c.upstream_group}]</span>
                          )}
                          <span style={{ opacity: 0.65, fontSize: 11 }}>({ c.rate ?? 1 }x · {logCount})</span>
                        </Tag>
                      );
                    })}
                  </div>

                  {/* Recharts 折线图容器 */}
                  <Spin spinning={anyLoadingLogs && summary.totalLogs === 0}>
                    <ErrorBoundary>
                      <div style={{ width: '100%', height: 250 }}>
                        <ResponsiveContainer width="100%" height="100%">
                          <LineChart
                            data={chartDataInfo.points}
                            margin={{ top: 10, right: 20, left: 0, bottom: 4 }}
                          >
                            <CartesianGrid strokeDasharray="3 3" stroke={gridStroke} vertical={false} />
                            <XAxis
                              dataKey="timeLabel"
                              stroke={axisColor}
                              tick={{ fill: axisColor, fontSize: 11 }}
                              tickLine={false}
                              axisLine={{ stroke: gridStroke }}
                            />
                            <YAxis
                              stroke={axisColor}
                              tick={{ fill: axisColor, fontSize: 11 }}
                              tickLine={false}
                              axisLine={false}
                              width={48}
                              domain={['auto', 'auto']}
                              tickFormatter={(v: number) => `${Number(v.toFixed(4))}x`}
                            />
                            <RechartsTooltip
                              content={({ active, payload }) => {
                                if (!active || !payload || payload.length === 0) return null;
                                const row = payload[0]?.payload || {};
                                if (chartDataInfo.mode === 'single') {
                                  const delta = Number(row.delta ?? 0);
                                  const hasConv = Boolean(
                                    row.upstream_currency &&
                                      row.site_currency &&
                                      String(row.upstream_currency).toLowerCase() !==
                                        String(row.site_currency).toLowerCase(),
                                  );
                                  return (
                                    <div
                                      style={{
                                        background: isLight ? '#ffffff' : '#1f1f1f',
                                        border: `1px solid ${borderColor}`,
                                        borderRadius: 8,
                                        padding: '8px 12px',
                                        boxShadow: '0 6px 20px rgba(0,0,0,0.15)',
                                        fontSize: 12,
                                        minWidth: 220,
                                      }}
                                    >
                                      <div style={{ fontWeight: 600, marginBottom: 4 }}>
                                        {row.configName} <Text type="secondary" style={{ fontSize: 11 }}>({row.configYid})</Text>
                                      </div>
                                      <div style={{ color: axisColor, fontSize: 11, marginBottom: 6 }}>
                                        {row.fullTime} · {row.isBaseline ? '当前配置基准' : (row.trigger_type === 'manual' ? '手动同步' : '定时同步')}
                                      </div>
                                      <div style={{ display: 'flex', flexDirection: 'column', gap: 3 }}>
                                        <div>
                                          同步分组：<Tag style={{ margin: 0, fontSize: 11 }}>{row.upstream_group}</Tag>
                                        </div>
                                        <div>
                                          上游原始分组倍率：<strong style={{ color: '#f59e0b' }}>{row.upstream_ratio}x</strong>
                                        </div>
                                        {hasConv && (
                                          <div>
                                            币种汇率折算：{row.upstream_currency} → {row.site_currency} (汇率 {row.currency_rate})
                                          </div>
                                        )}
                                        {Number(row.rate_add) > 0 && (
                                          <div>叠加增量：+{row.rate_add}</div>
                                        )}
                                        <div style={{ marginTop: 2, paddingTop: 4, borderTop: `1px dashed ${borderColor}` }}>
                                          本站生效渠道倍率：<strong style={{ color: '#1677ff', fontSize: 13 }}>{row.applied_rate}x</strong>
                                          {!row.isBaseline && (
                                            <span style={{ marginLeft: 6, fontSize: 11, color: delta !== 0 ? (delta > 0 ? '#ef4444' : '#10b981') : axisColor }}>
                                              ({delta !== 0 ? `原 ${row.previous_rate}x, ${delta > 0 ? `+${delta}` : delta}x` : '与上次一致'})
                                            </span>
                                          )}
                                        </div>
                                      </div>
                                    </div>
                                  );
                                }

                                return (
                                  <div
                                    style={{
                                      background: isLight ? '#ffffff' : '#1f1f1f',
                                      border: `1px solid ${borderColor}`,
                                      borderRadius: 8,
                                      padding: '8px 12px',
                                      boxShadow: '0 6px 20px rgba(0,0,0,0.15)',
                                      fontSize: 12,
                                      minWidth: 220,
                                    }}
                                  >
                                    <div style={{ fontWeight: 600, marginBottom: 6, color: axisColor }}>
                                      {row.fullTime}
                                    </div>
                                    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                                      {payload.map((entry: any) => {
                                        const cfgId = String(entry.dataKey || '').replace('cfg_', '');
                                        const meta = row[`meta_${cfgId}`];
                                        return (
                                          <div
                                            key={entry.dataKey}
                                            style={{
                                              display: 'flex',
                                              alignItems: 'center',
                                              justifyContent: 'space-between',
                                              gap: 10,
                                            }}
                                          >
                                            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 5 }}>
                                              <span
                                                style={{
                                                  width: 8,
                                                  height: 8,
                                                  borderRadius: '50%',
                                                  backgroundColor: entry.color,
                                                  display: 'inline-block',
                                                }}
                                              />
                                              <span>{entry.name}</span>
                                            </span>
                                            <span style={{ fontWeight: 600, color: entry.color }}>
                                              {entry.value}x
                                              {meta && meta.previous_rate !== undefined && Math.abs(meta.applied_rate - meta.previous_rate) > 0.00001
                                                ? ` (原 ${meta.previous_rate}x)`
                                                : ''}
                                            </span>
                                          </div>
                                        );
                                      })}
                                    </div>
                                  </div>
                                );
                              }}
                            />
                            <Legend wrapperStyle={{ fontSize: 12, paddingTop: 6 }} />
                            {chartDataInfo.mode === 'single' ? (
                              <>
                                <Line
                                  type="monotone"
                                  dataKey="applied_rate"
                                  name="本站生效渠道倍率"
                                  stroke="#1677ff"
                                  strokeWidth={2.5}
                                  dot={{ r: 3.5 }}
                                  activeDot={{ r: 6 }}
                                />
                                <Line
                                  type="monotone"
                                  dataKey="upstream_ratio"
                                  name="上游分组原始倍率"
                                  stroke="#f59e0b"
                                  strokeWidth={2}
                                  strokeDasharray="5 4"
                                  dot={{ r: 3 }}
                                  activeDot={{ r: 5 }}
                                />
                              </>
                            ) : (
                              chartDataInfo.series.map(s => (
                                <Line
                                  key={s.key}
                                  type="monotone"
                                  dataKey={s.key}
                                  name={s.name}
                                  stroke={s.color}
                                  strokeWidth={2.2}
                                  dot={{ r: 3 }}
                                  activeDot={{ r: 5 }}
                                  connectNulls
                                />
                              ))
                            )}
                          </LineChart>
                        </ResponsiveContainer>
                      </div>
                    </ErrorBoundary>
                  </Spin>
                </div>

                {/* 右侧：分组倍率变动与同步历史时间轴 */}
                <div
                  style={{
                    background: cardBg,
                    border: `1px solid ${borderColor}`,
                    borderRadius: 8,
                    padding: '12px 14px',
                    display: 'flex',
                    flexDirection: 'column',
                    minWidth: 0,
                  }}
                >
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      flexWrap: 'wrap',
                      gap: 8,
                      marginBottom: 10,
                    }}
                  >
                    <Space size={6}>
                      <ClockCircleOutlined style={{ color: '#1677ff', fontSize: 14 }} />
                      <span style={{ fontWeight: 600, fontSize: 13 }}>
                        分组倍率变动与同步历史记录
                      </span>
                      <Tag style={{ margin: 0, fontSize: 11 }}>{filteredHistoryLogs.length}</Tag>
                    </Space>
                    <Segmented
                      size="small"
                      value={historyFilter}
                      onChange={(val) => setHistoryFilter(val as 'all' | 'changed' | 'error')}
                      options={[
                        { label: `全部 (${mergedHistoryLogs.length})`, value: 'all' },
                        { label: `仅倍率变动 (${mergedHistoryLogs.filter(l => l.rateChanged).length})`, value: 'changed' },
                        { label: `异常 (${mergedHistoryLogs.filter(l => l.status !== 1).length})`, value: 'error' },
                      ]}
                    />
                  </div>

                  <div
                    style={{
                      maxHeight: 285,
                      overflowY: 'auto',
                      display: 'flex',
                      flexDirection: 'column',
                      gap: 6,
                      paddingRight: 2,
                    }}
                  >
                    {filteredHistoryLogs.length === 0 ? (
                      <div style={{ padding: '42px 12px', textAlign: 'center' }}>
                        <Text type="secondary" style={{ fontSize: 12 }}>
                          {mergedHistoryLogs.length === 0
                            ? '当前系统下的渠道暂无分组倍率同步历史记录（点击渠道卡片上的「立即同步」可拉取最新倍率并记录）'
                            : '当前筛选条件下暂无匹配的倍率同步记录'}
                        </Text>
                      </div>
                    ) : (
                      filteredHistoryLogs.slice(0, 60).map(log => {
                        const isSuccess = log.status === 1;
                        const upCurr = (log.upstream_currency || '').trim();
                        const stCurr = (log.site_currency || '').trim();
                        const hasConv = Boolean(upCurr && stCurr && upCurr.toLowerCase() !== stCurr.toLowerCase());
                        return (
                          <div
                            key={log.id}
                            style={{
                              padding: '7px 10px',
                              borderRadius: 6,
                              background: isSuccess
                                ? (log.rateChanged
                                    ? (isLight ? '#f0f7ff' : 'rgba(22, 119, 255, 0.12)')
                                    : subtleBg)
                                : (isLight ? '#fff2f0' : 'rgba(255, 77, 79, 0.12)'),
                              border: isSuccess
                                ? (log.rateChanged
                                    ? (isLight ? '1px solid #91caff' : '1px solid rgba(22, 119, 255, 0.35)')
                                    : `1px solid ${borderColor}`)
                                : (isLight ? '1px solid #ffccc7' : '1px solid rgba(255, 77, 79, 0.3)'),
                              display: 'flex',
                              flexDirection: 'column',
                              gap: 4,
                            }}
                          >
                            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 6, flexWrap: 'wrap' }}>
                              <Space size={4} wrap>
                                <Tag
                                  color={isSuccess ? 'success' : 'error'}
                                  style={{ margin: 0, fontSize: 10, padding: '0 4px', lineHeight: '15px', height: 17 }}
                                >
                                  {isSuccess ? '成功' : '失败'}
                                </Tag>
                                <span style={{ fontWeight: 600, fontSize: 12 }}>{log.configName}</span>
                                <Text type="secondary" style={{ fontSize: 11 }}>({log.configYid})</Text>
                                <Tag color="blue" style={{ margin: 0, fontSize: 10, padding: '0 5px', lineHeight: '15px', height: 17 }}>
                                  分组: {log.upstream_group || '-'}
                                </Tag>
                                <Tag
                                  color={log.trigger_type === 'manual' ? 'purple' : 'cyan'}
                                  style={{ margin: 0, fontSize: 10, padding: '0 4px', lineHeight: '15px', height: 17 }}
                                >
                                  {log.trigger_type === 'manual'
                                    ? '手动'
                                    : (log.sync_interval_minutes > 0 ? `定时(${log.sync_interval_minutes}分)` : '自动')}
                                </Tag>
                              </Space>
                              <Text type="secondary" style={{ fontSize: 11 }}>
                                {log.created_at ? formatApiDateTime(log.created_at) : '-'}
                              </Text>
                            </div>

                            {isSuccess ? (
                              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', flexWrap: 'wrap', gap: 6, fontSize: 11 }}>
                                <Space size={4} wrap>
                                  <span style={{ color: axisColor }}>
                                    上游倍率 <strong>{log.upstream_ratio}x</strong>
                                  </span>
                                  {hasConv && (
                                    <Tag color="geekblue" style={{ margin: 0, fontSize: 10, padding: '0 4px', lineHeight: '14px', height: 16 }}>
                                      {upCurr}→{stCurr} (÷{log.currency_rate})
                                    </Tag>
                                  )}
                                  {log.rate_add > 0 && (
                                    <Tag color="orange" style={{ margin: 0, fontSize: 10, padding: '0 4px', lineHeight: '14px', height: 16 }}>
                                      +{log.rate_add}
                                    </Tag>
                                  )}
                                </Space>
                                <Space size={4}>
                                  {log.rateChanged ? (
                                    <Tag
                                      color="blue"
                                      style={{ margin: 0, fontSize: 11, padding: '0 6px', fontWeight: 600 }}
                                    >
                                      {log.previous_rate}x <SwapRightOutlined /> {log.applied_rate}x (
                                      {log.rateDelta > 0 ? `+${log.rateDelta}` : log.rateDelta}x)
                                    </Tag>
                                  ) : (
                                    <Tag style={{ margin: 0, fontSize: 10, padding: '0 5px' }}>
                                      倍率保持 {log.applied_rate}x
                                    </Tag>
                                  )}
                                </Space>
                              </div>
                            ) : (
                              <div style={{ fontSize: 11, color: '#ff4d4f', wordBreak: 'break-all' }}>
                                {log.error_message || '同步失败'}
                              </div>
                            )}
                          </div>
                        );
                      })
                    )}
                  </div>
                </div>
              </div>

              {/* 3. 绑定该系统的各个上游渠道详细信息卡片矩阵 */}
              <div>
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    marginBottom: 8,
                  }}
                >
                  <Space size={6}>
                    <ThunderboltOutlined style={{ color: '#fa8c16', fontSize: 14 }} />
                    <span style={{ fontWeight: 600, fontSize: 13 }}>
                      绑定「{systemLabel}」的上游渠道详细配置与关联分组明细 ({configs.length})
                    </span>
                  </Space>
                  {effectiveSelectedId !== 'all' && (
                    <Button size="small" type="link" onClick={() => setSelectedConfigId('all')} style={{ padding: 0, height: 'auto', fontSize: 12 }}>
                      恢复显示全部渠道曲线
                    </Button>
                  )}
                </div>

                <div
                  style={{
                    display: 'grid',
                    gridTemplateColumns: 'repeat(auto-fill, minmax(330px, 1fr))',
                    gap: 10,
                  }}
                >
                  {configs.map((cfg, idx) => {
                    const isActive = (cfg.status ?? 1) === 1;
                    const isFocused = effectiveSelectedId === cfg.id;
                    const color = LINE_COLORS[idx % LINE_COLORS.length];
                    const logs = syncLogsMap[cfg.id] || [];
                    const latestSuccessLog = logs.find(l => l.status === 1);
                    const latestLog = logs[0];
                    const catIds = resolveCategoryIds(cfg);
                    const boundChannels = cfg.bound_channels || [];
                    const upCurr = (cfg.upstream_currency || '').trim();
                    const stCurr = (cfg.site_currency || '').trim();
                    const hasConv = Boolean(upCurr && stCurr && upCurr.toLowerCase() !== stCurr.toLowerCase());
                    const isSyncing = Boolean(syncTriggerLoadingMap[cfg.id]);
                    const canManualSync =
                      cfg.upstream_system === 'newapi' && Boolean((cfg.upstream_group || '').trim());

                    return (
                      <div
                        key={cfg.id}
                        style={{
                          background: cardBg,
                          border: isFocused
                            ? '1.5px solid #1677ff'
                            : `1px solid ${borderColor}`,
                          borderRadius: 8,
                          padding: '10px 12px',
                          display: 'flex',
                          flexDirection: 'column',
                          gap: 8,
                          boxShadow: isFocused ? '0 0 0 2px rgba(22, 119, 255, 0.15)' : undefined,
                          transition: 'all 0.15s ease',
                        }}
                      >
                        {/* 卡片头部：渠道名、YID、状态、快捷动作 */}
                        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
                          <div style={{ display: 'flex', alignItems: 'center', gap: 6, minWidth: 0, flex: 1 }}>
                            <span
                              style={{
                                width: 8,
                                height: 8,
                                borderRadius: '50%',
                                backgroundColor: color,
                                flexShrink: 0,
                              }}
                            />
                            <span
                              style={{
                                fontWeight: 600,
                                fontSize: 13,
                                overflow: 'hidden',
                                textOverflow: 'ellipsis',
                                whiteSpace: 'nowrap',
                              }}
                              title={cfg.name}
                            >
                              {cfg.name}
                            </span>
                            <Text keyboard style={{ fontSize: 11, color: '#1677ff', flexShrink: 0 }}>
                              YID {cfg.yid || '-'}
                            </Text>
                            <Tag
                              color={isActive ? 'success' : 'error'}
                              style={{ margin: 0, fontSize: 10, padding: '0 4px', lineHeight: '15px', height: 17, flexShrink: 0 }}
                            >
                              {isActive ? '激活' : '已禁用'}
                            </Tag>
                          </div>

                          <Space size={2} style={{ flexShrink: 0 }}>
                            <Tooltip title={isFocused ? '当前已在曲线图中聚焦此渠道' : '在上方曲线图中单独查看此渠道'}>
                              <Button
                                type={isFocused ? 'primary' : 'text'}
                                size="small"
                                icon={<LineChartOutlined />}
                                onClick={() => setSelectedConfigId(isFocused ? 'all' : cfg.id)}
                                style={{ height: 22, padding: '0 6px', fontSize: 11 }}
                              >
                                曲线
                              </Button>
                            </Tooltip>
                            {canManualSync && (
                              <Tooltip title="立即从 newapi 上游拉取并同步最新分组倍率">
                                <Button
                                  type="text"
                                  size="small"
                                  icon={<SyncOutlined spin={isSyncing} />}
                                  loading={isSyncing}
                                  onClick={() => onManualSyncRate(cfg)}
                                  style={{ height: 22, padding: '0 6px', fontSize: 11, color: '#1677ff' }}
                                >
                                  同步
                                </Button>
                              </Tooltip>
                            )}
                            <Tooltip title="编辑此上游渠道预设">
                              <Button
                                type="text"
                                size="small"
                                icon={<EditOutlined />}
                                onClick={() => onEditConfig(cfg)}
                                style={{ height: 22, width: 22, padding: 0 }}
                              />
                            </Tooltip>
                          </Space>
                        </div>

                        {/* 核心参数行：倍率、调度、服务商、分类 */}
                        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4, alignItems: 'center' }}>
                          <Tag color="orange" style={{ margin: 0, fontSize: 11, fontWeight: 600 }}>
                            当前倍率 {cfg.rate ?? 1}x
                          </Tag>
                          <Tag style={{ margin: 0, fontSize: 11 }}>
                            默认优先级 P{cfg.priority ?? 0} · 权重 W{cfg.weight ?? 1}
                          </Tag>
                          {cfg.provider_type && (
                            <Tag color="geekblue" style={{ margin: 0, fontSize: 11 }}>
                              服务商: {cfg.provider_type}
                            </Tag>
                          )}
                          {catIds.map(cid => {
                            const cname = resolveCategoryName(cid);
                            return cname ? (
                              <Tag key={cid} style={{ margin: 0, fontSize: 11 }}>
                                {cname}
                              </Tag>
                            ) : null;
                          })}
                        </div>

                        {/* Base URL 与密钥信息 */}
                        <div
                          style={{
                            fontSize: 11,
                            padding: '5px 8px',
                            borderRadius: 6,
                            background: subtleBg,
                            display: 'flex',
                            flexDirection: 'column',
                            gap: 4,
                          }}
                        >
                          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
                            <Text
                              code
                              style={{
                                fontSize: 11,
                                overflow: 'hidden',
                                textOverflow: 'ellipsis',
                                whiteSpace: 'nowrap',
                                maxWidth: 230,
                              }}
                              title={cfg.base_url}
                            >
                              {cfg.base_url || '-'}
                            </Text>
                            <span style={{ fontSize: 11, flexShrink: 0, color: cfg.has_api_key || cfg.api_key ? '#10b981' : '#f59e0b' }}>
                              {cfg.has_api_key || cfg.api_key ? (
                                <><CheckCircleOutlined /> 密钥已配</>
                              ) : (
                                <><CloseCircleOutlined /> 未配密钥</>
                              )}
                            </span>
                          </div>

                          {/* 上游分组与倍率换算配置明细 */}
                          {(cfg.upstream_system === 'newapi' || cfg.upstream_group) && (
                            <div
                              style={{
                                display: 'flex',
                                flexWrap: 'wrap',
                                alignItems: 'center',
                                gap: 4,
                                paddingTop: 4,
                                borderTop: `1px dashed ${borderColor}`,
                              }}
                            >
                              <Tag color="blue" style={{ margin: 0, fontSize: 10, padding: '0 5px' }}>
                                绑定分组: {cfg.upstream_group || '未选分组'}
                              </Tag>
                              {latestSuccessLog && (
                                <Tag color="gold" style={{ margin: 0, fontSize: 10, padding: '0 5px' }}>
                                  上游分组倍率: {latestSuccessLog.upstream_ratio}x
                                </Tag>
                              )}
                              {hasConv && (
                                <Tag color="geekblue" style={{ margin: 0, fontSize: 10, padding: '0 5px' }}>
                                  {upCurr}→{stCurr} (汇率 {cfg.upstream_currency_rate || 7.2})
                                </Tag>
                              )}
                              {Number(cfg.upstream_sync_rate_add) > 0 && (
                                <Tag color="orange" style={{ margin: 0, fontSize: 10, padding: '0 5px' }}>
                                  增量 +{cfg.upstream_sync_rate_add}
                                </Tag>
                              )}
                              <Tag color="cyan" style={{ margin: 0, fontSize: 10, padding: '0 5px' }}>
                                {Number(cfg.upstream_sync_interval_minutes) > 0
                                  ? `每 ${cfg.upstream_sync_interval_minutes} 分钟同步`
                                  : '手动同步'}
                              </Tag>
                              {cfg.upstream_synced_at && (
                                <span style={{ color: axisColor, fontSize: 10 }}>
                                  上次同步: {formatApiDateTime(cfg.upstream_synced_at)}
                                </span>
                              )}
                              {latestLog && latestLog.status !== 1 && (
                                <Tooltip title={latestLog.error_message || '最近一次同步失败'}>
                                  <Tag color="error" style={{ margin: 0, fontSize: 10, padding: '0 5px' }}>
                                    最近同步异常
                                  </Tag>
                                </Tooltip>
                              )}
                            </div>
                          )}
                        </div>

                        {/* 额度与下游绑定模型分组明细 */}
                        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', flexWrap: 'wrap', gap: 6, fontSize: 11 }}>
                          <span style={{ color: axisColor }}>
                            总额度: <strong>{formatQuotaSummary(cfg.quota_used, cfg.quota_limit)}</strong>
                            {Number(cfg.daily_quota_limit ?? -1) >= 0 && (
                              <> · 日: <strong>{formatQuotaSummary(cfg.daily_quota_used, cfg.daily_quota_limit)}</strong></>
                            )}
                          </span>

                          {boundChannels.length === 0 ? (
                            <Text type="secondary" style={{ fontSize: 11 }}>未绑定模型分组</Text>
                          ) : (
                            <Popover
                              placement="bottomRight"
                              trigger={['hover', 'click']}
                              title={
                                <div style={{ fontSize: 12, fontWeight: 600 }}>
                                  「{cfg.name}」绑定的模型渠道分组 ({boundChannels.length})
                                </div>
                              }
                              content={
                                <div style={{ maxHeight: 220, overflowY: 'auto', display: 'flex', flexDirection: 'column', gap: 5, minWidth: 260 }}>
                                  {boundChannels.map(ch => (
                                    <div
                                      key={`${ch.id}-${ch.is_ha ? 'ha' : 'direct'}`}
                                      style={{
                                        display: 'flex',
                                        alignItems: 'center',
                                        justifyContent: 'space-between',
                                        gap: 8,
                                        padding: '4px 8px',
                                        borderRadius: 5,
                                        background: subtleBg,
                                        border: `1px solid ${borderColor}`,
                                        fontSize: 12,
                                      }}
                                    >
                                      <Space size={4} style={{ minWidth: 0 }}>
                                        <span style={{ fontWeight: 500 }}>{ch.name}</span>
                                        <Tag color={ch.is_ha ? 'purple' : 'default'} style={{ margin: 0, fontSize: 10, padding: '0 4px' }}>
                                          {ch.is_ha ? 'HA' : '直连'}
                                        </Tag>
                                        <Tag style={{ margin: 0, fontSize: 10, padding: '0 4px' }}>
                                          P{ch.effective_priority ?? cfg.priority ?? 0} / W{ch.effective_weight ?? cfg.weight ?? 1}
                                        </Tag>
                                      </Space>
                                      <Link
                                        to={channelEditPath(adminPath, ch.id)}
                                        style={{ fontSize: 11, color: '#1677ff', flexShrink: 0 }}
                                      >
                                        编辑
                                      </Link>
                                    </div>
                                  ))}
                                </div>
                              }
                            >
                              <Tag
                                color="processing"
                                style={{ margin: 0, fontSize: 11, cursor: 'pointer', padding: '0 6px' }}
                              >
                                <LinkOutlined style={{ marginRight: 3 }} />
                                已绑 {boundChannels.length} 个模型分组
                              </Tag>
                            </Popover>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
};

export default UpstreamSystemDetailPanel;
