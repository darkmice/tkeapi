/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useEffect, useState } from 'react';
import { Card, Col, DatePicker, Row, Spin, Typography } from 'antd';
import { useTranslation } from 'react-i18next';
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import dayjs from 'dayjs';
import ErrorBoundary from '../../components/ErrorBoundary';
import useSettingsStore from '../../store/settings';
import request from '../../utils/request';

const { Title, Text } = Typography;

interface DateCount {
  date: string;
  count: number;
}

interface RankItem {
  name: string;
  amount: number;
}

interface UserAnalytics {
  daily_registrations: DateCount[];
  monthly_registrations: DateCount[];
  daily_consumption_rank: RankItem[];
  monthly_consumption_rank: RankItem[];
  monthly_recharge_rank: RankItem[];
  total_recharge_rank: RankItem[];
}

const EMPTY: UserAnalytics = {
  daily_registrations: [],
  monthly_registrations: [],
  daily_consumption_rank: [],
  monthly_consumption_rank: [],
  monthly_recharge_rank: [],
  total_recharge_rank: [],
};

const AXIS_TICK = { fontSize: 12, fill: 'var(--muted-foreground)' };

function formatMoney(value: number, symbol: string): string {
  const n = Number(value) || 0;
  const abs = Math.abs(n);
  if (abs >= 1_000_000) return `${symbol}${(n / 1_000_000).toFixed(1)}M`;
  if (abs >= 10_000) return `${symbol}${(n / 1_000).toFixed(1)}K`;
  return `${symbol}${n.toLocaleString('zh-CN', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
}

function shortName(value: string): string {
  return value.length > 10 ? `${value.slice(0, 10)}…` : value;
}

function ChartFrame({ children }: { children: React.ReactElement }) {
  return (
    <div className="h-[280px] w-full [&_.recharts-cartesian-axis-tick_text]:fill-[var(--muted-foreground)] [&_.recharts-responsive-container]:outline-none [&_.recharts-surface]:outline-none [&_.recharts-wrapper]:outline-none [&_.recharts-wrapper:focus]:outline-none [&_.recharts-wrapper:focus-visible]:outline-none">
      <ResponsiveContainer width="100%" height="100%">
        {children}
      </ResponsiveContainer>
    </div>
  );
}

function ChartTooltip({
  active,
  payload,
  label,
  titleKey,
  formatValue,
}: {
  active?: boolean;
  payload?: ReadonlyArray<{ value?: number | string; payload?: Record<string, string> }>;
  label?: string | number;
  titleKey: 'date' | 'name';
  formatValue: (n: number) => string;
}) {
  if (!active || !payload?.length) return null;
  const row = payload[0];
  const title = row?.payload?.[titleKey] || (label != null ? String(label) : '');
  return (
    <div
      style={{
        background: 'var(--popover)',
        color: 'var(--popover-foreground)',
        border: '1px solid var(--border-custom)',
        borderRadius: 8,
        padding: '6px 10px',
        boxShadow: '0 8px 24px rgba(0,0,0,0.12)',
        fontSize: 12,
      }}
    >
      <div style={{ marginBottom: 2, color: 'var(--muted-foreground)' }}>{title}</div>
      <div style={{ fontWeight: 600 }}>{formatValue(Number(row?.value ?? 0))}</div>
    </div>
  );
}

function EmptyHint({ text }: { text: string }) {
  return (
    <div className="flex h-[280px] items-center justify-center">
      <Text type="secondary">{text}</Text>
    </div>
  );
}

function AnalyticsCard({
  title,
  desc,
  children,
}: {
  title: string;
  desc: string;
  children: React.ReactNode;
}) {
  return (
    <Card variant="borderless" style={{ borderRadius: 12 }} styles={{ body: { padding: '16px 12px 8px' } }}>
      <div className="px-2 pb-2">
        <div style={{ fontSize: 14, fontWeight: 500 }}>{title}</div>
        <Text type="secondary" style={{ fontSize: 12 }}>{desc}</Text>
      </div>
      <ErrorBoundary>{children}</ErrorBoundary>
    </Card>
  );
}

const UserAnalytics: React.FC = () => {
  const { t } = useTranslation();
  const { settings } = useSettingsStore();
  const currencySymbol = settings?.currency?.currency_symbol || '￥';
  const [data, setData] = useState<UserAnalytics>(EMPTY);
  const [loading, setLoading] = useState(true);
  const [rankMonth, setRankMonth] = useState(() => dayjs().startOf('month'));

  useEffect(() => {
    let cancelled = false;
    (async () => {
      setLoading(true);
      try {
        const res = (await request.get('/users/analytics', {
          params: { month: rankMonth.format('YYYY-MM') },
        })) as UserAnalytics;
        if (!cancelled && res) {
          setData({
            daily_registrations: res.daily_registrations || [],
            monthly_registrations: res.monthly_registrations || [],
            daily_consumption_rank: res.daily_consumption_rank || [],
            monthly_consumption_rank: res.monthly_consumption_rank || [],
            monthly_recharge_rank: res.monthly_recharge_rank || [],
            total_recharge_rank: res.total_recharge_rank || [],
          });
        }
      } catch (error) {
        console.error('Failed to fetch user analytics:', error);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [rankMonth]);

  const people = (n: number) => t('user_analytics.people', { count: n, defaultValue: `${n} 人` });
  const money = (n: number) => formatMoney(n, currencySymbol);
  const empty = t('user_analytics.empty', '暂无数据');

  return (
    <div>
      <div style={{ marginBottom: 16, display: 'flex', justifyContent: 'space-between', alignItems: 'flex-end', gap: 12, flexWrap: 'wrap' }}>
        <div>
          <Title level={4} style={{ margin: 0 }}>
            {t('menu.user_analytics', '用户数据分析')}
          </Title>
          <Text type="secondary">
            {t('user_analytics.subtitle', '每日注册展示最近 10 天，月注册展示最近 12 个月，排行榜默认取前 10 名')}
          </Text>
        </div>
        <div>
          <div style={{ fontSize: 12, marginBottom: 4, color: 'var(--muted-foreground)' }}>
            {t('user_analytics.rank_month', '月排行')}
          </div>
          <DatePicker
            picker="month"
            value={rankMonth}
            allowClear={false}
            inputReadOnly
            disabledDate={(current) => !!current && current.isAfter(dayjs(), 'month')}
            onChange={(value) => {
              if (value) setRankMonth(value.startOf('month'));
            }}
          />
        </div>
      </div>
      <Spin spinning={loading}>
        <Row gutter={[16, 16]}>
          <Col xs={24} xl={12}>
            <AnalyticsCard
              title={t('user_analytics.daily_reg', '每日注册')}
              desc={t('user_analytics.daily_reg_desc', '最近 10 天新注册用户')}
            >
              <ChartFrame>
                <AreaChart data={data.daily_registrations} margin={{ left: 0, right: 12, top: 8 }} accessibilityLayer={false}>
                  <defs>
                    <linearGradient id="userRegFill" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="5%" stopColor="var(--chart-1)" stopOpacity={0.8} />
                      <stop offset="95%" stopColor="var(--chart-1)" stopOpacity={0.05} />
                    </linearGradient>
                  </defs>
                  <CartesianGrid vertical={false} stroke="var(--border-custom)" />
                  <XAxis dataKey="date" tickLine={false} axisLine={false} tickMargin={8} tick={AXIS_TICK} tickFormatter={(v: string) => dayjs(v).format('MM-DD')} />
                  <YAxis domain={[0, 'auto']} allowDecimals={false} tickLine={false} axisLine={false} width={36} tick={AXIS_TICK} />
                  <Tooltip cursor={false} content={(props) => <ChartTooltip active={props.active} payload={props.payload as never} label={props.label} titleKey="date" formatValue={people} />} />
                  <Area dataKey="count" type="monotone" baseValue={0} fill="url(#userRegFill)" stroke="var(--chart-1)" strokeWidth={2} />
                </AreaChart>
              </ChartFrame>
            </AnalyticsCard>
          </Col>

          <Col xs={24} xl={12}>
            <AnalyticsCard
              title={t('user_analytics.monthly_reg', '最近12个月新注册用户')}
              desc={t('user_analytics.monthly_reg_desc', '含本月，按自然月统计')}
            >
              <ChartFrame>
                <BarChart data={data.monthly_registrations} margin={{ left: 0, right: 12, top: 8 }} accessibilityLayer={false}>
                  <CartesianGrid vertical={false} stroke="var(--border-custom)" />
                  <XAxis dataKey="date" tickLine={false} axisLine={false} tickMargin={8} interval={0} tick={AXIS_TICK} tickFormatter={(v: string) => (v.length >= 7 ? v.slice(2) : v)} />
                  <YAxis allowDecimals={false} tickLine={false} axisLine={false} width={36} tick={AXIS_TICK} />
                  <Tooltip cursor={false} content={(props) => <ChartTooltip active={props.active} payload={props.payload as never} label={props.label} titleKey="date" formatValue={people} />} />
                  <Bar dataKey="count" fill="var(--chart-2)" radius={[4, 4, 0, 0]} maxBarSize={28} />
                </BarChart>
              </ChartFrame>
            </AnalyticsCard>
          </Col>

          <Col xs={24} xl={12}>
            <AnalyticsCard
              title={t('user_analytics.daily_spend', '每日消费排行')}
              desc={t('user_analytics.daily_spend_desc', '今日平台消费前 10 名')}
            >
              {data.daily_consumption_rank.length === 0 ? <EmptyHint text={empty} /> : (
                <ChartFrame>
                  <BarChart data={data.daily_consumption_rank} layout="vertical" margin={{ left: 8, right: 16, top: 4, bottom: 4 }} accessibilityLayer={false}>
                    <CartesianGrid horizontal={false} stroke="var(--border-custom)" />
                    <XAxis type="number" tickLine={false} axisLine={false} tick={AXIS_TICK} tickFormatter={(v: number) => formatMoney(v, currencySymbol)} />
                    <YAxis type="category" dataKey="name" width={112} interval={0} tickLine={false} axisLine={false} tick={AXIS_TICK} tickFormatter={shortName} />
                    <Tooltip cursor={false} content={(props) => <ChartTooltip active={props.active} payload={props.payload as never} label={props.label} titleKey="name" formatValue={money} />} />
                    <Bar dataKey="amount" fill="var(--chart-3)" radius={[0, 4, 4, 0]} maxBarSize={18} />
                  </BarChart>
                </ChartFrame>
              )}
            </AnalyticsCard>
          </Col>

          <Col xs={24} xl={12}>
            <AnalyticsCard
              title={t('user_analytics.monthly_spend', '月平台消费前 10')}
              desc={rankMonth.format('YYYY-MM')}
            >
              {data.monthly_consumption_rank.length === 0 ? <EmptyHint text={empty} /> : (
                <ChartFrame>
                  <BarChart data={data.monthly_consumption_rank} layout="vertical" margin={{ left: 8, right: 16, top: 4, bottom: 4 }} accessibilityLayer={false}>
                    <CartesianGrid horizontal={false} stroke="var(--border-custom)" />
                    <XAxis type="number" tickLine={false} axisLine={false} tick={AXIS_TICK} tickFormatter={(v: number) => formatMoney(v, currencySymbol)} />
                    <YAxis type="category" dataKey="name" width={112} interval={0} tickLine={false} axisLine={false} tick={AXIS_TICK} tickFormatter={shortName} />
                    <Tooltip cursor={false} content={(props) => <ChartTooltip active={props.active} payload={props.payload as never} label={props.label} titleKey="name" formatValue={money} />} />
                    <Bar dataKey="amount" fill="var(--chart-5)" radius={[0, 4, 4, 0]} maxBarSize={18} />
                  </BarChart>
                </ChartFrame>
              )}
            </AnalyticsCard>
          </Col>

          <Col xs={24} xl={12}>
            <AnalyticsCard
              title={t('user_analytics.monthly_recharge', '月充值金额前 10')}
              desc={rankMonth.format('YYYY-MM')}
            >
              {data.monthly_recharge_rank.length === 0 ? <EmptyHint text={empty} /> : (
                <ChartFrame>
                  <BarChart data={data.monthly_recharge_rank} layout="vertical" margin={{ left: 8, right: 16, top: 4, bottom: 4 }} accessibilityLayer={false}>
                    <CartesianGrid horizontal={false} stroke="var(--border-custom)" />
                    <XAxis type="number" tickLine={false} axisLine={false} tick={AXIS_TICK} tickFormatter={(v: number) => formatMoney(v, currencySymbol)} />
                    <YAxis type="category" dataKey="name" width={112} interval={0} tickLine={false} axisLine={false} tick={AXIS_TICK} tickFormatter={shortName} />
                    <Tooltip cursor={false} content={(props) => <ChartTooltip active={props.active} payload={props.payload as never} label={props.label} titleKey="name" formatValue={money} />} />
                    <Bar dataKey="amount" fill="var(--chart-4)" radius={[0, 4, 4, 0]} maxBarSize={18} />
                  </BarChart>
                </ChartFrame>
              )}
            </AnalyticsCard>
          </Col>

          <Col xs={24} xl={12}>
            <AnalyticsCard
              title={t('user_analytics.total_recharge', '充值总和排行')}
              desc={t('user_analytics.total_recharge_desc', '累计充值金额前 10 名')}
            >
              {data.total_recharge_rank.length === 0 ? <EmptyHint text={empty} /> : (
                <ChartFrame>
                  <BarChart data={data.total_recharge_rank} layout="vertical" margin={{ left: 8, right: 16, top: 4, bottom: 4 }} accessibilityLayer={false}>
                    <CartesianGrid horizontal={false} stroke="var(--border-custom)" />
                    <XAxis type="number" tickLine={false} axisLine={false} tick={AXIS_TICK} tickFormatter={(v: number) => formatMoney(v, currencySymbol)} />
                    <YAxis type="category" dataKey="name" width={112} interval={0} tickLine={false} axisLine={false} tick={AXIS_TICK} tickFormatter={shortName} />
                    <Tooltip cursor={false} content={(props) => <ChartTooltip active={props.active} payload={props.payload as never} label={props.label} titleKey="name" formatValue={money} />} />
                    <Bar dataKey="amount" fill="var(--chart-2)" radius={[0, 4, 4, 0]} maxBarSize={18} />
                  </BarChart>
                </ChartFrame>
              )}
            </AnalyticsCard>
          </Col>
        </Row>
      </Spin>
    </div>
  );
};

export default UserAnalytics;
