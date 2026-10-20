/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useMemo, useState } from 'react';
import { Button, Spin, Typography } from 'antd';
import { ArrowLeftOutlined } from '@ant-design/icons';
import { TrendingDown, TrendingUp } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
} from 'recharts';
import dayjs from 'dayjs';
import type { ModelTrend30dResponse } from '../../types';

const { Title } = Typography;

const CHART_COLORS = [
  'var(--chart-1)',
  'var(--chart-2)',
  'var(--chart-3)',
  'var(--chart-4)',
  'var(--chart-5)',
];

type DayPoint = { date: string; cost: number };
type OverviewPoint = { date: string; requests: number; cost: number };
type OverviewKey = 'requests' | 'cost';

function emptyDayMap(): Map<string, { requests: number; cost: number }> {
  const dataMap = new Map<string, { requests: number; cost: number }>();
  for (let i = 29; i >= 0; i--) {
    dataMap.set(dayjs().subtract(i, 'day').format('YYYY-MM-DD'), { requests: 0, cost: 0 });
  }
  return dataMap;
}

function buildOverviewSeries(data: ModelTrend30dResponse | null): OverviewPoint[] {
  const dataMap = emptyDayMap();
  if (data?.daily_data && Array.isArray(data.daily_data)) {
    data.daily_data.forEach((item) => {
      const prev = dataMap.get(item.date) || { requests: 0, cost: 0 };
      prev.requests += item.count || 0;
      prev.cost += item.total_cost || 0;
      dataMap.set(item.date, prev);
    });
  }
  return Array.from(dataMap.entries())
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([date, v]) => ({
      date: date.slice(5),
      requests: v.requests,
      cost: parseFloat(v.cost.toFixed(6)),
    }));
}

function buildDailySeries(data: ModelTrend30dResponse | null, model: string): DayPoint[] {
  const dataMap = new Map<string, number>();
  for (let i = 29; i >= 0; i--) {
    dataMap.set(dayjs().subtract(i, 'day').format('YYYY-MM-DD'), 0);
  }
  if (data?.daily_data && Array.isArray(data.daily_data)) {
    data.daily_data.forEach((item) => {
      if (item.model !== model) return;
      dataMap.set(item.date, parseFloat(item.total_cost.toFixed(6)));
    });
  }
  return Array.from(dataMap.entries())
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([date, cost]) => ({ date: date.slice(5), cost }));
}

function weekTrendPercent(series: DayPoint[]): number | null {
  if (series.length < 14) return null;
  const last = series.slice(-7).reduce((sum, d) => sum + d.cost, 0);
  const prev = series.slice(-14, -7).reduce((sum, d) => sum + d.cost, 0);
  if (prev === 0) return last > 0 ? 100 : 0;
  return ((last - prev) / prev) * 100;
}

function ChartTooltipBox({
  active,
  payload,
  label,
  valueLabel,
  formatValue,
}: {
  active?: boolean;
  payload?: Array<{ value?: number; color?: string; fill?: string }>;
  label?: string;
  valueLabel: string;
  formatValue: (value: number) => string;
}) {
  if (!active || !payload?.length) return null;
  const item = payload[0];
  const value = Number(item.value ?? 0);
  return (
    <div className="grid min-w-[9rem] gap-1.5 rounded-lg border border-border/50 bg-background px-2.5 py-1.5 text-xs shadow-xl">
      <div className="font-medium text-foreground">{label}</div>
      <div className="flex items-center gap-2">
        <span
          className="h-2.5 w-2.5 shrink-0 rounded-[2px]"
          style={{ backgroundColor: item.fill || item.color }}
        />
        <span className="text-muted-foreground">{valueLabel}</span>
        <span className="ml-auto font-medium tabular-nums text-foreground">
          {formatValue(value)}
        </span>
      </div>
    </div>
  );
}

const ModelDistribution30d: React.FC<{
  data: ModelTrend30dResponse | null;
  loading: boolean;
  currencySymbol: string;
  onBack: () => void;
}> = ({ data, loading, currencySymbol, onBack }) => {
  const { t } = useTranslation();
  const costLabel = t('dashboard.estimated_cost');
  const requestsLabel = t('dashboard.total_requests');
  const models = data?.top_models;
  const [activeOverview, setActiveOverview] = useState<OverviewKey>('requests');

  const overviewSeries = useMemo(() => buildOverviewSeries(data), [data]);
  const overviewTotals = useMemo(() => ({
    requests: overviewSeries.reduce((sum, d) => sum + d.requests, 0),
    cost: overviewSeries.reduce((sum, d) => sum + d.cost, 0),
  }), [overviewSeries]);

  const seriesByModel = useMemo(() => {
    if (!models?.length) return {};
    const map: Record<string, DayPoint[]> = {};
    models.forEach((m) => {
      map[m.model] = buildDailySeries(data, m.model);
    });
    return map;
  }, [data, models]);

  return (
    <div style={{ animation: 'fadeIn 0.3s' }}>
      <div style={{ marginBottom: 24 }}>
        <Button icon={<ArrowLeftOutlined />} onClick={onBack}>{t('common.back', '返回')}</Button>
        <Title level={3} style={{ margin: '16px 0 0' }}>
          {t('dashboard.model_distribution_30d', '模型分布 (按成本) - 最近 30 天趋势')}
        </Title>
      </div>

      {models && Array.isArray(models) && models.length > 0 ? (
        <>
        <div className="mb-4 flex flex-col rounded-xl border border-border bg-[#fafafa] text-card-foreground shadow-sm dark:bg-[#141414]">
          <div className="flex flex-col items-stretch border-b border-border sm:flex-row">
            <div className="flex flex-1 flex-col justify-center gap-1 px-6 py-5">
              <div className="text-base font-semibold leading-none tracking-tight">
                {t('dashboard.trend_30d_title', '最近 30 天数据趋势')}
              </div>
              <p className="m-0 text-sm text-muted-foreground">
                {t('dashboard.trend_30d_desc', '按请求次数与预估成本查看最近 30 天消耗')}
              </p>
            </div>
            <div className="flex">
              {([
                { key: 'requests' as const, label: requestsLabel, value: overviewTotals.requests.toLocaleString() },
                { key: 'cost' as const, label: costLabel, value: `${currencySymbol}${parseFloat(overviewTotals.cost.toFixed(6))}` },
              ]).map((item) => (
                <button
                  key={item.key}
                  type="button"
                  data-active={activeOverview === item.key}
                  className="relative flex flex-1 flex-col justify-center gap-1 border-t border-border px-6 py-4 text-left even:border-l sm:border-t-0 sm:px-8 sm:py-6 data-[active=true]:bg-muted/50"
                  onClick={() => setActiveOverview(item.key)}
                >
                  <span className="text-xs text-muted-foreground">{item.label}</span>
                  <span className="text-lg font-bold leading-none sm:text-3xl">{item.value}</span>
                </button>
              ))}
            </div>
          </div>
          <div className="px-2 pt-4 pb-4 sm:p-6">
            <div className="h-[250px] w-full">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart accessibilityLayer data={overviewSeries} margin={{ left: 12, right: 12 }}>
                  <CartesianGrid vertical={false} stroke="var(--border-custom)" />
                  <XAxis
                    dataKey="date"
                    tickLine={false}
                    axisLine={false}
                    tickMargin={8}
                    minTickGap={24}
                    tick={{ fontSize: 10, fill: 'var(--muted-foreground)' }}
                  />
                  <Tooltip
                    cursor={false}
                    content={(props) => (
                      <ChartTooltipBox
                        active={props.active}
                        payload={props.payload as unknown as Array<{ value?: number; color?: string; fill?: string }>}
                        label={props.label as string | undefined}
                        valueLabel={activeOverview === 'cost' ? costLabel : requestsLabel}
                        formatValue={(v) =>
                          activeOverview === 'cost'
                            ? `${currencySymbol}${parseFloat(v.toFixed(6))}`
                            : Math.round(v).toLocaleString()
                        }
                      />
                    )}
                  />
                  <Bar
                    dataKey={activeOverview}
                    fill={activeOverview === 'cost' ? 'var(--chart-2)' : 'var(--chart-1)'}
                    radius={4}
                  />
                </BarChart>
              </ResponsiveContainer>
            </div>
          </div>
        </div>

        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2 xl:grid-cols-3">
          {models.map((m, index) => {
            const fill = CHART_COLORS[index % CHART_COLORS.length];
            const series = seriesByModel[m.model] || [];
            const trend = weekTrendPercent(series);
            const up = trend != null && trend >= 0;
            return (
              <div
                key={m.model}
                className="flex flex-col rounded-xl border border-border bg-[#fafafa] text-card-foreground shadow-sm dark:bg-[#141414]"
              >
                <div className="flex flex-col gap-1 px-6 pt-6">
                  <div className="truncate text-base font-semibold leading-none tracking-tight">
                    {m.model}
                  </div>
                  <p className="m-0 text-sm text-muted-foreground">
                    {t('dashboard.last_30_days', '最近 30 天')}
                  </p>
                </div>
                <div className="px-2 pt-4 pb-0">
                  <div className="h-[180px] w-full">
                    <ResponsiveContainer width="100%" height="100%">
                      <BarChart accessibilityLayer data={series} margin={{ top: 8, left: 8, right: 8, bottom: 0 }}>
                        <CartesianGrid vertical={false} stroke="var(--border-custom)" />
                        <XAxis
                          dataKey="date"
                          tickLine={false}
                          axisLine={false}
                          tickMargin={8}
                          minTickGap={16}
                          tick={{ fontSize: 11, fill: 'var(--muted-foreground)' }}
                        />
                        <Tooltip
                          cursor={false}
                          content={(props) => (
                            <ChartTooltipBox
                              active={props.active}
                              payload={props.payload as unknown as Array<{ value?: number; color?: string; fill?: string }>}
                              label={props.label as string | undefined}
                              valueLabel={costLabel}
                              formatValue={(v) => `${currencySymbol}${parseFloat(v.toFixed(6))}`}
                            />
                          )}
                        />
                        <Bar dataKey="cost" fill={fill} radius={4} maxBarSize={28} />
                      </BarChart>
                    </ResponsiveContainer>
                  </div>
                </div>
                <div className="flex flex-col items-start gap-1 px-6 pt-2 pb-6 text-[11px]">
                  {trend != null && Math.abs(trend) >= 0.05 && (
                    <div className="flex items-center gap-1.5 font-medium leading-none">
                      {up
                        ? t('dashboard.cost_trend_up', { pct: Math.abs(trend).toFixed(1), defaultValue: `近 7 日成本上升 ${Math.abs(trend).toFixed(1)}%` })
                        : t('dashboard.cost_trend_down', { pct: Math.abs(trend).toFixed(1), defaultValue: `近 7 日成本下降 ${Math.abs(trend).toFixed(1)}%` })}
                      {up ? <TrendingUp className="size-3" /> : <TrendingDown className="size-3" />}
                    </div>
                  )}
                  <div className="text-muted-foreground leading-none">
                    {t('dashboard.total_spend', '总花费')} {currencySymbol}{parseFloat(Number(m.total_cost).toFixed(6))}
                    <span className="mx-1.5">·</span>
                    {t('dashboard.call_count', '调用次数')} {m.count}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
        </>
      ) : (
        <div className="flex min-h-[400px] items-center justify-center text-muted-foreground">
          {loading ? <Spin size="large" /> : t('dashboard.no_data')}
        </div>
      )}
    </div>
  );
};

export default ModelDistribution30d;
