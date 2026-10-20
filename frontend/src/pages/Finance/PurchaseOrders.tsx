/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useCallback, useEffect, useState } from 'react';
import { Table, Card, Typography, Space, Input, Button, Tag, Grid, List, Select } from 'antd';
import { ShoppingOutlined, SyncOutlined, SearchOutlined } from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import type { Dayjs } from 'dayjs';
import request from '../../utils/request';
import useSettingsStore from '../../store/settings';
import { formatApiDateTime } from '../../utils/timedisplay';
import { toTimeRangeParams } from '../../utils/dateRangeParams';
import LogDateTimeRangePicker from '../../components/LogDateTimeRangePicker';
import { listPagination, useListPager } from '../../components/ListPagination';

const { Title, Text } = Typography;

interface PurchaseOrder {
  id: number;
  order_no: string;
  kind: string;
  username: string;
  uid: string;
  plan_name: string;
  price_paid: number;
  pay_method: string;
  status: string;
  tier_applied: number;
  tier_reverted: number;
  tier_note: string;
  tier_expires_at: string | null;
  face_value: number | null;
  credit_remaining: number | null;
  credit_status: string | null;
  created_at: string;
}

const KIND: Record<string, { color?: string; label: string }> = {
  coupon_pack: { color: 'purple', label: '卡券加油包' },
  coupon_subscription: { color: 'blue', label: '卡券会员订阅' },
  game_ticket: { color: 'gold', label: '营销小游戏' },
  membership: { label: '会员卡券' },
};

const PAY: Record<string, string> = {
  balance: '现金余额',
  wechat: '微信',
  alipay: '支付宝',
  stripe: 'Stripe',
  hyperbc: 'HyperBC',
  bonuspay: 'BonusPay',
  allinpay_wechat: '通联微信',
  allinpay_alipay: '通联支付宝',
  admin: '管理员发放',
};

const ORDER_STATUS: Record<string, { color: string; label: string }> = {
  unpaid: { color: 'warning', label: '未付款' },
  paid: { color: 'success', label: '已付款' },
  refunded: { color: 'default', label: '已退回' },
  closed: { color: 'default', label: '已关闭' },
};

const CREDIT_STATUS: Record<string, string> = {
  active: '可用',
  exhausted: '已用完',
  expired: '已过期',
  refunded: '已退回',
};

const mono: React.CSSProperties = {
  display: 'inline-flex',
  alignItems: 'center',
  whiteSpace: 'nowrap',
  fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
  fontSize: 11,
  letterSpacing: '-0.2px',
};

function tierText(row: PurchaseOrder): string {
  if (row.kind === 'game_ticket') return '—';
  if (row.tier_applied !== 1) return row.tier_note || '等级未变更';
  if (row.tier_reverted === 1) return '等级已结束';
  const expiry = row.tier_expires_at ? formatApiDateTime(row.tier_expires_at, 'YYYY-MM-DD') : '长期';
  return row.tier_note || `已变更，${expiry}`;
}

function creditText(row: PurchaseOrder): string {
  if (row.face_value == null) return '—';
  const status = CREDIT_STATUS[row.credit_status || ''] || row.credit_status || '';
  if (row.kind === 'game_ticket') {
    const remain = row.credit_remaining == null ? '—' : Math.round(row.credit_remaining);
    const total = Math.round(row.face_value);
    return `${remain} / ${total} 张${status ? ` ${status}` : ''}`;
  }
  const remain = row.credit_remaining == null ? '—' : row.credit_remaining.toFixed(2);
  return `${remain} / ${row.face_value.toFixed(2)}${status ? ` ${status}` : ''}`;
}

const PurchaseOrders: React.FC = () => {
  const { t } = useTranslation();
  const screens = Grid.useBreakpoint();
  const { settings } = useSettingsStore();
  const currencySymbol = settings?.currency?.currency_symbol || '¥';
  const [data, setData] = useState<PurchaseOrder[]>([]);
  const [loading, setLoading] = useState(true);
  const [total, setTotal] = useState(0);
  const [totalAmount, setTotalAmount] = useState(0);
  const { page, pageSize, onChange } = useListPager();
  const [search, setSearch] = useState('');
  const [kindFilter, setKindFilter] = useState<string>('');
  const [dateRange, setDateRange] = useState<[Dayjs, Dayjs] | null>(null);

  const fetchData = useCallback(async () => {
    setLoading(true);
    try {
      const resp = await (request.get('/finance/purchase-orders', {
        params: {
          page,
          per_page: pageSize,
          user_id: search || undefined,
          kind: kindFilter || undefined,
          ...toTimeRangeParams(dateRange),
        },
        skipErrorHandler: true,
      } as any) as unknown as Promise<{ data: PurchaseOrder[]; total: number; total_amount: number }>);
      setData(resp?.data || []);
      setTotal(resp?.total || 0);
      setTotalAmount(resp?.total_amount || 0);
    } catch {
      setData([]);
      setTotal(0);
      setTotalAmount(0);
    } finally {
      setLoading(false);
    }
  }, [page, pageSize, search, kindFilter, dateRange]);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const columns = [
    {
      title: t('finance.order_no', { defaultValue: '订单号' }),
      dataIndex: 'order_no',
      width: 220,
      render: (text: string) => <Text copyable style={mono}>{text || '—'}</Text>,
    },
    {
      title: t('finance.user_info'),
      width: 150,
      render: (_: unknown, row: PurchaseOrder) => (
        <Space vertical size={0}>
          <Text strong>{row.username || '—'}</Text>
          <Text type="secondary" style={{ fontSize: 12 }}>{row.uid ? `UID: ${row.uid}` : '—'}</Text>
        </Space>
      ),
    },
    {
      title: '类型',
      dataIndex: 'kind',
      width: 120,
      render: (kind: string) => {
        const item = KIND[kind];
        return <Tag color={item?.color}>{item?.label || kind}</Tag>;
      },
    },
    { title: '商品', dataIndex: 'plan_name', width: 160 },
    {
      title: '支付',
      dataIndex: 'pay_method',
      width: 100,
      render: (method: string) => PAY[method] || method,
    },
    {
      title: '金额',
      dataIndex: 'price_paid',
      width: 120,
      render: (val: number) => (
        <Text strong style={{ color: '#ff4d4f' }}>{currencySymbol}{Number(val || 0).toFixed(6)}</Text>
      ),
    },
    {
      title: '状态',
      dataIndex: 'status',
      width: 90,
      render: (status: string) => {
        const item = ORDER_STATUS[status];
        return <Tag color={item?.color || 'default'}>{item?.label || status}</Tag>;
      },
    },
    { title: '等级', width: 220, render: (_: unknown, row: PurchaseOrder) => tierText(row) },
    { title: '剩余 / 面值', width: 160, render: (_: unknown, row: PurchaseOrder) => creditText(row) },
    {
      title: '时间',
      dataIndex: 'created_at',
      width: 170,
      render: (value: string) => formatApiDateTime(value),
    },
  ];

  return (
    <Card bordered={false}>
      <div style={{ display: 'flex', flexDirection: screens.xs ? 'column' : 'row', flexWrap: 'wrap', justifyContent: 'space-between', marginBottom: 12, alignItems: screens.xs ? 'flex-start' : 'center', gap: 16 }}>
        <Space size="small" align="center" wrap>
          <ShoppingOutlined style={{ fontSize: 24, color: '#1677ff' }} />
          <Title level={2} style={{ margin: 0, fontSize: screens.xs ? 20 : 24 }}>
            {t('finance.purchase_order_title', { defaultValue: '订单明细' })}
          </Title>
          <Text type="secondary" style={{ marginLeft: screens.xs ? 0 : 8 }}>
            实付合计: <Text strong style={{ color: '#1677ff', fontSize: 16 }}>{currencySymbol}{totalAmount.toFixed(6)}</Text>
          </Text>
        </Space>
        <Space wrap style={{ width: screens.xs ? '100%' : 'auto' }}>
          <LogDateTimeRangePicker value={dateRange} onChange={setDateRange} isAdmin className="font-size-12" />
          <Select
            allowClear
            placeholder="按类型筛选"
            value={kindFilter || undefined}
            onChange={(val) => setKindFilter(val || '')}
            style={{ width: 140 }}
            options={[
              { value: 'coupon_pack', label: '卡券加油包' },
              { value: 'coupon_subscription', label: '卡券会员订阅' },
              { value: 'game_ticket', label: '营销小游戏' },
            ]}
          />
          <Input
            placeholder="搜索 用户名 / UID / 订单号 / 商品"
            prefix={<SearchOutlined />}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onPressEnter={fetchData}
            style={{ width: 260 }}
          />
          <Button icon={<SyncOutlined />} onClick={fetchData}>{t('common.refresh')}</Button>
        </Space>
      </div>
      <Text type="secondary" style={{ display: 'block', marginBottom: 12 }}>
        站点内直接购买消费订单（含卡券加油包、卡券会员订阅、营销小游戏购券等）。在线充值在「在线充值明细」，兑换码核销不是订单。
      </Text>
      {screens.xs ? (
        <List
          dataSource={data}
          loading={loading}
          pagination={listPagination({ total, current: page, pageSize, onChange, size: 'small' })}
          renderItem={(row) => (
            <List.Item style={{ padding: '0 0 8px 0', border: 'none' }}>
              <Card
                size="small"
                style={{ width: '100%', borderRadius: 8 }}
                title={<Text strong>{row.plan_name}</Text>}
                extra={<Tag color={ORDER_STATUS[row.status]?.color || 'default'}>{ORDER_STATUS[row.status]?.label || row.status}</Tag>}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                  <Text type="secondary" style={{ fontSize: 12 }}>类型</Text>
                  <Tag color={KIND[row.kind]?.color} style={{ margin: 0 }}>{KIND[row.kind]?.label || row.kind}</Tag>
                </div>
                <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                  <Text type="secondary" style={{ fontSize: 12 }}>订单号</Text>
                  <Text copyable style={mono}>{row.order_no}</Text>
                </div>
                <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                  <Text type="secondary" style={{ fontSize: 12 }}>用户</Text>
                  <Text style={{ fontSize: 12 }}>{row.username || '—'} {row.uid ? `· ${row.uid}` : ''}</Text>
                </div>
                <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                  <Text type="secondary" style={{ fontSize: 12 }}>金额</Text>
                  <Text strong>{currencySymbol}{Number(row.price_paid || 0).toFixed(6)}</Text>
                </div>
                <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                  <Text type="secondary" style={{ fontSize: 12 }}>等级</Text>
                  <Text style={{ fontSize: 12 }}>{tierText(row)}</Text>
                </div>
                <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                  <Text type="secondary" style={{ fontSize: 12 }}>时间</Text>
                  <Text style={{ fontSize: 12 }}>{formatApiDateTime(row.created_at)}</Text>
                </div>
              </Card>
            </List.Item>
          )}
        />
      ) : (
        <Table
          rowKey={(row) => `${row.kind}_${row.id}`}
          className="compact-table"
          loading={loading}
          dataSource={data}
          columns={columns}
          size="small"
          scroll={{ x: 'max-content' }}
          pagination={listPagination({ total, current: page, pageSize, onChange })}
        />
      )}
    </Card>
  );
};

export default PurchaseOrders;
