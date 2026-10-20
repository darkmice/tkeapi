/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useCallback, useEffect, useState } from 'react';
import { Table, Card, Typography, Space, Input, Button, Tag, Select, Grid, List } from 'antd';
import { listPagination, useListPager } from '../../components/ListPagination';
import { SyncOutlined, SearchOutlined, BarChartOutlined } from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import request from '../../utils/request';
import useSettingsStore from '../../store/settings';
import { formatApiDateTime } from '../../utils/timedisplay';
import { defaultMonthRange, toTimeRangeParams } from '../../utils/dateRangeParams';
import LogDateTimeRangePicker from '../../components/LogDateTimeRangePicker';
import type { Dayjs } from 'dayjs';

const { Title, Text } = Typography;

interface OrderRecord {
  id: number;
  out_trade_no: string;
  user_id: string;
  username: string;
  uid: string;
  payment_method: string;
  amount: number;
  status: string;
  trade_no: string | null;
  created_at: string;
  paid_at: string | null;
}

const statusMap: Record<string, { color: string; label: string }> = {
  pending: { color: 'processing', label: '待支付' },
  paid: { color: 'success', label: '已支付' },
  closed: { color: 'default', label: '已关闭' },
};

const methodMap: Record<string, { color: string; label: string }> = {
  wechat: { color: '#07c160', label: '微信支付' },
  alipay: { color: '#1677ff', label: '支付宝' },
  stripe: { color: '#635bff', label: 'Stripe' },
  bonuspay: { color: '#ff6a00', label: 'BonusPay' },
  hyperbc: { color: '#8b5cf6', label: 'HyperBC' },
  allinpay_wechat: { color: '#07c160', label: '通联微信' },
  'allinpay wechat': { color: '#07c160', label: '通联微信' },
  allinpay_alipay: { color: '#1677ff', label: '通联支付宝' },
  'allinpay alipay': { color: '#1677ff', label: '通联支付宝' },
};

const methodOrder = [
  'wechat',
  'alipay',
  'allinpay_wechat',
  'allinpay_alipay',
  'stripe',
  'hyperbc',
  'bonuspay',
];

const formatOrderTime = (
  timeStr: string | null,
  _refTimeStr: string | null,
  _outTradeNo: string,
  _isCreatedAt: boolean
): string => {
  if (!timeStr) return '';
  // timesystem=UTC 落库后统一按 timedisplay 渲染；不再做订单号字面量纠偏
  return formatApiDateTime(timeStr);
};

const OrderDetails: React.FC = () => {
  const { t } = useTranslation();
  const screens = Grid.useBreakpoint();
  const { settings } = useSettingsStore();
  const currencySymbol = settings?.currency?.currency_symbol || '¥';
  const [data, setData] = useState<OrderRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [total, setTotal] = useState(0);
  const [totalAmount, setTotalAmount] = useState<number>(0);
  const [paymentMethods, setPaymentMethods] = useState<string[]>([]);
  const { page, pageSize, onChange } = useListPager();
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<string | undefined>();
  const [methodFilter, setMethodFilter] = useState<string | undefined>();
  const [dateRange, setDateRange] = useState<[Dayjs, Dayjs] | null>(() => defaultMonthRange());

  const fetchData = useCallback(async () => {
    setLoading(true);
    try {
      const resp = await (request.get('/finance/orders', {
        params: {
          page,
          per_page: pageSize,
          user_id: search || undefined,
          status: statusFilter,
          payment_method: methodFilter,
          ...toTimeRangeParams(dateRange),
        }
      }) as unknown as Promise<{ data: OrderRecord[]; total: number; total_amount: number; payment_methods?: string[] }>);
      setData(resp.data);
      setTotal(resp.total);
      setTotalAmount(resp.total_amount || 0);
      if (Array.isArray(resp.payment_methods)) {
        setPaymentMethods(resp.payment_methods);
      } else if (!methodFilter) {
        setPaymentMethods(
          Array.from(new Set((resp.data || []).map((r) => r.payment_method).filter(Boolean)))
        );
      }
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, [page, pageSize, search, statusFilter, methodFilter, dateRange]);

  const methodOptions = React.useMemo(() => {
    const normalizeMethod = (m: string) => {
      if (m === 'allinpay wechat') return 'allinpay_wechat';
      if (m === 'allinpay alipay') return 'allinpay_alipay';
      return m;
    };
    const seen = new Set<string>();
    const list: string[] = [];
    for (const raw of paymentMethods) {
      const m = normalizeMethod(raw);
      if (m && !seen.has(m)) {
        seen.add(m);
        list.push(m);
      }
    }
    list.sort((a, b) => {
      const idxA = methodOrder.indexOf(a);
      const idxB = methodOrder.indexOf(b);
      if (idxA !== -1 && idxB !== -1) return idxA - idxB;
      if (idxA !== -1) return -1;
      if (idxB !== -1) return 1;
      return a.localeCompare(b);
    });
    return list.map((m) => ({
      label: methodMap[m]?.label || m,
      value: m,
    }));
  }, [paymentMethods]);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const columns = [
    {
      title: t('finance.order_no', { defaultValue: '订单号' }),
      dataIndex: 'out_trade_no',
      key: 'out_trade_no',
      width: 220,
      render: (text: string) => (
        <Text
          copyable
          style={{
            display: 'inline-flex',
            alignItems: 'center',
            whiteSpace: 'nowrap',
            fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
            fontSize: 11,
            letterSpacing: '-0.2px',
          }}
        >
          {text}
        </Text>
      ),
    },
    {
      title: t('finance.user_info'),
      key: 'user',
      width: 150,
      render: (_: unknown, record: OrderRecord) => (
        <Space vertical size={0}>
          <Text strong>{record.username || '-'}</Text>
          <Text type="secondary" style={{ fontSize: 12 }}>
            {record.uid ? `UID: ${record.uid}` : '-'}
          </Text>
        </Space>
      ),
    },
    {
      title: '支付方式',
      dataIndex: 'payment_method',
      key: 'payment_method',
      width: 100,
      render: (method: string) => {
        const m = methodMap[method] || { color: 'default', label: method };
        return <Tag color={m.color}>{m.label}</Tag>;
      },
    },
    {
      title: '金额',
      dataIndex: 'amount',
      key: 'amount',
      width: 140,
      render: (val: number) => <Text strong style={{ color: '#ff4d4f', whiteSpace: 'nowrap' }}>{currencySymbol}{val.toFixed(6)}</Text>,
    },
    {
      title: t('finance.status'),
      dataIndex: 'status',
      key: 'status',
      width: 100,
      render: (status: string) => {
        const s = statusMap[status] || { color: 'default', label: status };
        return <Tag color={s.color}>{s.label}</Tag>;
      },
    },
    {
      title: '第三方交易号',
      dataIndex: 'trade_no',
      key: 'trade_no',
      render: (text: string | null) => text ? (
        <Text
          copyable
          style={{
            display: 'inline-flex',
            alignItems: 'center',
            whiteSpace: 'nowrap',
            fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
            fontSize: 11,
            letterSpacing: '-0.2px',
          }}
        >
          {text}
        </Text>
      ) : <Text type="secondary" style={{ fontSize: 11 }}>-</Text>,
    },
    {
      title: '创建时间',
      dataIndex: 'created_at',
      key: 'created_at',
      width: 180,
      render: (text: string, record: OrderRecord) => formatOrderTime(text, record.paid_at, record.out_trade_no, true),
    },
    {
      title: '支付时间',
      dataIndex: 'paid_at',
      key: 'paid_at',
      width: 180,
      render: (text: string | null, record: OrderRecord) => text ? formatOrderTime(text, record.created_at, record.out_trade_no, false) : <Text type="secondary">-</Text>,
    },
  ];

  return (
    <Card bordered={false}>
      <div style={{ display: 'flex', flexDirection: screens.xs ? 'column' : 'row', flexWrap: 'wrap', justifyContent: 'space-between', marginBottom: 12, alignItems: screens.xs ? 'flex-start' : 'center', gap: 16 }}>
        <Space size="small" align="center" wrap>
          <BarChartOutlined style={{ fontSize: 24, color: '#52c41a' }} />
          <Title level={2} style={{ margin: 0, fontSize: screens.xs ? 20 : 24 }}>{t('finance.order_title')}</Title>
          <Text type="secondary" style={{ marginLeft: screens.xs ? 0 : 8 }}>
            (已支付) 合计: <Text strong style={{ color: '#52c41a', fontSize: 16 }}>{currencySymbol}{totalAmount.toFixed(6)}</Text>
          </Text>
        </Space>
        <Space wrap style={{ width: screens.xs ? '100%' : 'auto' }}>
          <LogDateTimeRangePicker
            value={dateRange}
            onChange={setDateRange}
            isAdmin
            className="font-size-12"
          />
          <Select
            placeholder="支付状态"
            allowClear
            value={statusFilter}
            onChange={setStatusFilter}
            style={{ width: 120 }}
            options={[
              { label: '待支付', value: 'pending' },
              { label: '已支付', value: 'paid' },
              { label: '已关闭', value: 'closed' },
            ]}
          />
          <Select
            placeholder="支付方式"
            allowClear
            value={methodFilter}
            onChange={setMethodFilter}
            style={{ width: 120 }}
            options={methodOptions}
          />
          <Input
            placeholder="搜索 用户名 / UID"
            prefix={<SearchOutlined />}
            value={search}
            onChange={e => setSearch(e.target.value)}
            onPressEnter={fetchData}
            style={{ width: 200 }}
          />
          <Button icon={<SyncOutlined />} onClick={fetchData}>{t('common.refresh')}</Button>
        </Space>
      </div>

      {screens.xs ? (
        <List
          dataSource={data}
          loading={loading}
          pagination={listPagination({
            total,
            current: page,
            pageSize,
            onChange,
            size: "small",
          })}
          renderItem={(record) => {
            const statusInfo = statusMap[record.status] || { color: 'default', label: record.status };
            const methodInfo = methodMap[record.payment_method] || { color: 'default', label: record.payment_method };
            return (
              <List.Item style={{ padding: '0 0 8px 0', border: 'none' }}>
                <Card 
                  size="small" 
                  style={{ width: '100%', borderRadius: 8, boxShadow: '0 1px 2px rgba(0,0,0,0.05)' }}
                  title={<Text strong>{record.username || '-'}</Text>}
                  extra={<Tag color={statusInfo.color}>{statusInfo.label}</Tag>}
                >
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 }}>
                    <Text type="secondary" style={{ fontSize: 12 }}>订单号</Text>
                    <Text
                      copyable
                      style={{
                        display: 'inline-flex',
                        alignItems: 'center',
                        whiteSpace: 'nowrap',
                        fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
                        fontSize: 11,
                        letterSpacing: '-0.2px',
                      }}
                    >
                      {record.out_trade_no}
                    </Text>
                  </div>
                  <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                    <Text type="secondary" style={{ fontSize: 12 }}>UID</Text>
                    <Text style={{ fontSize: 12 }}>{record.uid || '-'}</Text>
                  </div>
                  <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                    <Text type="secondary" style={{ fontSize: 12 }}>支付方式</Text>
                    <Tag color={methodInfo.color} style={{ margin: 0 }}>{methodInfo.label}</Tag>
                  </div>
                  <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                    <Text type="secondary" style={{ fontSize: 12 }}>金额</Text>
                    <Text strong style={{ color: '#ff4d4f' }}>{currencySymbol}{record.amount.toFixed(6)}</Text>
                  </div>
                  {record.trade_no && (
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 }}>
                      <Text type="secondary" style={{ fontSize: 12 }}>第三方交易号</Text>
                      <Text
                        copyable
                        style={{
                          display: 'inline-flex',
                          alignItems: 'center',
                          whiteSpace: 'nowrap',
                          fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
                          fontSize: 11,
                          letterSpacing: '-0.2px',
                        }}
                      >
                        {record.trade_no}
                      </Text>
                    </div>
                  )}
                  <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                    <Text type="secondary" style={{ fontSize: 12 }}>创建时间</Text>
                    <Text style={{ fontSize: 12 }}>{formatOrderTime(record.created_at, record.paid_at, record.out_trade_no, true)}</Text>
                  </div>
                  {record.paid_at && (
                    <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 0 }}>
                      <Text type="secondary" style={{ fontSize: 12 }}>支付时间</Text>
                      <Text style={{ fontSize: 12 }}>{formatOrderTime(record.paid_at, record.created_at, record.out_trade_no, false)}</Text>
                    </div>
                  )}
                </Card>
              </List.Item>
            );
          }}
        />
      ) : (
        <Table
        dataSource={data}
        columns={columns}
        rowKey="id"
        className="compact-table"
        loading={loading}
        pagination={listPagination({
          total,
          current: page,
          pageSize,
          onChange,
        })}
        size="small"
        scroll={{ x: 'max-content' }}
      />
      )}
    </Card>
  );
};

export default OrderDetails;
