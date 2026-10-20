/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import {
  Button,
  Empty,
  Input,
  Modal,
  Pagination,
  Segmented,
  Spin,
  Table,
  Tag,
  Typography,
  message,
} from 'antd';
import {
  SearchOutlined,
  ShoppingOutlined,
  FileTextOutlined,
  ClockCircleOutlined,
  CheckCircleOutlined,
  WalletOutlined,
  CreditCardOutlined,
  EyeOutlined,
  WechatOutlined,
  AlipayCircleOutlined,
  CopyOutlined,
  RightOutlined,
} from '@ant-design/icons';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import request from '../../utils/request';
import useSettingsStore from '../../store/settings';
import { fetchActivePlugins } from '../../utils/activePlugins';
import { formatApiDateTime } from '../../utils/timedisplay';
import { listPagination, useListPager } from '../../components/ListPagination';
import { formatCreditMoney } from './money';
import PageHeader from '../../components/PageHeader';
import { offerReturnState } from './storefront/path';
import './credits.css';

type Order = {
  id: number;
  order_no: string;
  kind: string;
  plan_name: string;
  price_paid: number;
  pay_method: string;
  status: string;
  tier_applied: number;
  tier_reverted: number;
  tier_note: string;
  tier_expires_at: string | null;
  pay_expires_at: string | null;
  face_value: number | null;
  credit_remaining: number | null;
  credit_status: string | null;
  created_at: string;
};

const PAY_METHOD_MAP: Record<string, { labelZh: string; labelEn: string; icon?: React.ReactNode }> = {
  balance: { labelZh: '账户余额', labelEn: 'Balance', icon: <WalletOutlined className="text-[11px]" /> },
  wechat: { labelZh: '微信支付', labelEn: 'WeChat', icon: <WechatOutlined className="text-[11px]" /> },
  alipay: { labelZh: '支付宝', labelEn: 'Alipay', icon: <AlipayCircleOutlined className="text-[11px]" /> },
  stripe: { labelZh: 'Stripe', labelEn: 'Stripe', icon: <CreditCardOutlined className="text-[11px]" /> },
  hyperbc: { labelZh: 'HyperBC', labelEn: 'HyperBC', icon: <WalletOutlined className="text-[11px]" /> },
  allinpay_wechat: { labelZh: '通联微信', labelEn: 'Allinpay WeChat', icon: <WechatOutlined className="text-[11px]" /> },
  allinpay_alipay: { labelZh: '通联支付宝', labelEn: 'Allinpay Alipay', icon: <AlipayCircleOutlined className="text-[11px]" /> },
};

const getPayMethodInfo = (payMethod?: string | null, status?: string) => {
  if (status === 'unpaid' || !payMethod) {
    return {
      labelZh: '未支付',
      labelEn: 'Unpaid',
      icon: <ClockCircleOutlined className="text-[11px]" />,
    };
  }
  return (
    PAY_METHOD_MAP[payMethod] || {
      labelZh: payMethod || '未支付',
      labelEn: payMethod || 'Unpaid',
    }
  );
};

export function PayCountdown({
  expiresAt,
  en,
  onExpire,
}: {
  expiresAt?: string | null;
  en: boolean;
  onExpire: () => void;
}) {
  const [now, setNow] = useState(() => Date.now());
  const fired = React.useRef(false);
  const left = expiresAt ? new Date(expiresAt).getTime() - now : 0;

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    if (!expiresAt || left > 0 || fired.current) return;
    fired.current = true;
    onExpire();
  }, [expiresAt, left, onExpire]);

  if (!expiresAt) return null;
  if (left <= 0) {
    return (
      <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-zinc-200 dark:bg-zinc-800 text-zinc-500 border border-zinc-300 dark:border-zinc-700 whitespace-nowrap shrink-0">
        <ClockCircleOutlined className="text-[9px]" />
        {en ? 'Expired' : '已超时'}
      </span>
    );
  }

  const hours = Math.floor(left / 3600000);
  const minutes = Math.floor((left % 3600000) / 60000);
  const seconds = Math.floor((left % 60000) / 1000);
  const text = `${hours}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;

  return (
    <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-zinc-100 dark:bg-zinc-800 text-foreground border border-zinc-300 dark:border-zinc-700 whitespace-nowrap shrink-0">
      <ClockCircleOutlined className="text-[9px]" />
      {en ? `Left ${text}` : `剩余 ${text}`}
    </span>
  );
}

const MyOrders: React.FC = () => {
  const { i18n } = useTranslation();
  const en = (i18n.language || '').toLowerCase().startsWith('en');
  const { settings } = useSettingsStore();
  const symbol = settings?.currency?.currency_symbol || '$';

  const [rows, setRows] = useState<Order[]>([]);
  const [total, setTotal] = useState(0);
  const [totalAmount, setTotalAmount] = useState(0);
  const [loading, setLoading] = useState(false);
  const [membershipOn, setMembershipOn] = useState(false);
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<'all' | 'unpaid' | 'paid' | 'closed'>('all');
  const navigate = useNavigate();
  const location = useLocation();
  const { page, pageSize, onChange } = useListPager();

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const plugins = await fetchActivePlugins().catch(() => null);
      const pluginOn = !!(plugins?.active_plugins || []).some((p: any) => p.name === 'marketing_membership');
      setMembershipOn(pluginOn);
      if (!pluginOn) {
        setRows([]);
        setTotal(0);
        setTotalAmount(0);
        return;
      }
      const res = (await request.get('/plugins/marketing_membership/my-orders', {
        params: { page, per_page: pageSize, user_id: search || undefined },
        skipErrorHandler: true,
      } as any)) as { data: Order[]; total: number; total_amount?: number };
      setRows(res?.data || []);
      setTotal(res?.total || 0);
      setTotalAmount(res?.total_amount || 0);
    } catch {
      setRows([]);
      setTotal(0);
      setTotalAmount(0);
    } finally {
      setLoading(false);
    }
  }, [page, pageSize, search]);

  useEffect(() => {
    load();
  }, [load]);

  const unpaidCount = useMemo(() => rows.filter((r) => r.status === 'unpaid').length, [rows]);
  const paidCount = useMemo(() => rows.filter((r) => r.status === 'paid').length, [rows]);
  const closedCount = useMemo(() => rows.filter((r) => r.status === 'closed').length, [rows]);

  const displayRows = useMemo(() => {
    if (statusFilter === 'all') return rows;
    return rows.filter((r) => r.status === statusFilter);
  }, [rows, statusFilter]);

  const cancelOrder = (row: Order) => {
    Modal.confirm({
      centered: true,
      title: en ? 'Cancel this unpaid order?' : '取消这笔未付款订单？',
      content: en
        ? `Order #${row.order_no} (${row.plan_name}) will be closed.`
        : `确认取消订单 #${row.order_no}（${row.plan_name}）？取消后无法恢复。`,
      okText: en ? 'Confirm Cancel' : '确认取消',
      okButtonProps: { danger: true },
      cancelText: en ? 'Back' : '暂不取消',
      onOk: async () => {
        try {
          await request.post(`/plugins/marketing_membership/orders/${row.id}/cancel`);
          message.success(en ? 'Order closed' : '订单已成功取消');
          await load();
        } catch (err: any) {
          message.error(err?.message || (en ? 'Failed to cancel order' : '取消订单失败'));
        }
      },
    });
  };

  return (
    <div className="max-w-6xl mx-auto px-3 sm:px-4 py-2.5 sm:py-4 space-y-3 sm:space-y-3.5">
      {/* 统一标准页面头部 */}
      <PageHeader
        title={en ? 'Orders' : '订单管理'}
        description={
          en
            ? 'View and manage your membership and coupon orders. Unpaid orders expire in 24 hours.'
            : '查看与管理您的会员及卡券购买订单。未付款订单请在 24 小时内完成支付，超时将自动取消并释放库存。'
        }
        extra={
          membershipOn ? (
            <Button
              type="primary"
              icon={<ShoppingOutlined />}
              onClick={() => navigate('/credits', { state: offerReturnState(`${location.pathname}${location.search}${location.hash}`) })}
              size="middle"
              className="font-medium w-full sm:w-auto"
            >
              {en ? 'Browse Plans' : '选购会员方案'}
            </Button>
          ) : null
        }
        marginBottom={16}
      />

      {/* 紧凑 KPI 状态条 (黑白灰高对比、无多余说教文字) */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-2 sm:gap-2.5">
        <div className="rounded-lg border border-border bg-card p-2.5 sm:p-3 shadow-2xs">
          <div className="flex items-center justify-between text-muted-foreground text-[11px] sm:text-xs font-medium">
            <span>{en ? 'Total Orders' : '全部订单'}</span>
            <FileTextOutlined className="text-xs sm:text-sm opacity-60" />
          </div>
          <div className="mt-1 text-xl sm:text-2xl font-bold tracking-tight text-foreground truncate">
            {total}
          </div>
        </div>

        <div className={`rounded-lg border bg-card p-2.5 sm:p-3 shadow-2xs ${unpaidCount > 0 ? 'border-amber-500/40 dark:border-amber-500/30' : 'border-border'}`}>
          <div className="flex items-center justify-between text-muted-foreground text-[11px] sm:text-xs font-medium">
            <span>{en ? 'Pending Payment' : '待付款订单'}</span>
            <ClockCircleOutlined className={`text-xs sm:text-sm ${unpaidCount > 0 ? 'text-amber-500 opacity-90' : 'opacity-60'}`} />
          </div>
          <div className="mt-1 text-xl sm:text-2xl font-bold tracking-tight text-foreground flex items-baseline gap-1.5 truncate">
            <span className={unpaidCount > 0 ? 'text-amber-600 dark:text-amber-400' : ''}>{unpaidCount}</span>
            {unpaidCount > 0 && (
              <span className="text-[10px] sm:text-xs font-medium text-amber-600/80 dark:text-amber-400/80">
                ({en ? 'Pending' : '待付'})
              </span>
            )}
          </div>
        </div>

        <div className="rounded-lg border border-border bg-card p-2.5 sm:p-3 shadow-2xs">
          <div className="flex items-center justify-between text-muted-foreground text-[11px] sm:text-xs font-medium">
            <span>{en ? 'Completed Orders' : '已完成订单'}</span>
            <CheckCircleOutlined className="text-xs sm:text-sm opacity-60" />
          </div>
          <div className="mt-1 text-xl sm:text-2xl font-bold tracking-tight text-foreground truncate">
            {paidCount}
          </div>
        </div>

        <div className="rounded-lg border border-border bg-card p-2.5 sm:p-3 shadow-2xs">
          <div className="flex items-center justify-between text-muted-foreground text-[11px] sm:text-xs font-medium">
            <span>{en ? 'Total Spent' : '累计支付金额'}</span>
            <WalletOutlined className="text-xs sm:text-sm opacity-60" />
          </div>
          <div className="mt-1 text-xl sm:text-2xl font-bold tracking-tight text-foreground truncate" title={formatCreditMoney(symbol, totalAmount)}>
            {formatCreditMoney(symbol, totalAmount)}
          </div>
        </div>
      </div>

      {/* 紧凑筛选与搜索工具栏 (自适应手机与桌面) */}
      <div className="rounded-lg border border-border bg-card p-2 sm:p-2.5 shadow-2xs flex flex-col md:flex-row items-stretch md:items-center justify-between gap-2">
        <div className="w-full md:w-auto overflow-x-auto scrollbar-none">
          <Segmented
            size="small"
            block
            className="w-full md:w-auto"
            value={statusFilter}
            onChange={(val) => setStatusFilter(val as any)}
            options={[
              { label: `${en ? 'All' : '全部'} (${rows.length})`, value: 'all' },
              { label: `${en ? 'Unpaid' : '待付款'} (${unpaidCount})`, value: 'unpaid' },
              { label: `${en ? 'Paid' : '已付款'} (${paidCount})`, value: 'paid' },
              { label: `${en ? 'Closed' : '已关闭'} (${closedCount})`, value: 'closed' },
            ]}
          />
        </div>
        <div className="flex items-center gap-1.5 sm:gap-2">
          <Input
            size="small"
            allowClear
            prefix={<SearchOutlined className="text-muted-foreground text-xs" />}
            placeholder={en ? 'Search order no. / plan...' : '搜索订单号 / 方案名称...'}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onPressEnter={load}
            className="flex-1 md:w-60 h-8"
          />
          <Button size="small" onClick={load} className="h-8 px-3 shrink-0">{en ? 'Search' : '查询'}</Button>
        </div>
      </div>

      {/* 移动端专享卡片列表 (Mobile Card View) */}
      <div className="block md:hidden space-y-2.5">
        {loading && rows.length === 0 ? (
          <div className="py-12 flex justify-center items-center">
            <Spin size="default" />
          </div>
        ) : displayRows.length === 0 ? (
          <div className="rounded-lg border border-border bg-card p-6 text-center shadow-2xs">
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description={<span className="text-xs text-muted-foreground">{en ? 'No orders found' : '暂无订单记录'}</span>}
            >
              {membershipOn && (
                <Button
                  type="primary"
                  size="small"
                  icon={<ShoppingOutlined />}
                  onClick={() => navigate('/credits', { state: offerReturnState(`${location.pathname}${location.search}${location.hash}`) })}
                  className="font-medium"
                >
                  {en ? 'Browse Plans' : '选购方案'}
                </Button>
              )}
            </Empty>
          </div>
        ) : (
          displayRows.map((row) => {
            const methodInfo = getPayMethodInfo(row.pay_method, row.status);
            return (
              <div
                key={row.id}
                onClick={() => navigate(`/orders/${row.order_no}`)}
                className="rounded-xl border border-border bg-card p-3.5 shadow-2xs transition-all active:scale-[0.99] cursor-pointer flex flex-col gap-2.5 hover:border-zinc-300 dark:hover:border-zinc-700"
              >
                {/* 顶部行：订单号 + 复制 + 状态 Tag */}
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-1.5 min-w-0">
                    <span className="text-xs font-medium text-muted-foreground truncate max-w-[160px] xs:max-w-[200px]">
                      #{row.order_no}
                    </span>
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        navigator.clipboard.writeText(row.order_no);
                        message.success(en ? 'Order No. copied' : '订单号已复制');
                      }}
                      className="p-1 -m-1 text-muted-foreground/70 hover:text-foreground inline-flex items-center justify-center cursor-pointer transition-colors"
                      title={en ? 'Copy order number' : '复制订单号'}
                    >
                      <CopyOutlined className="text-xs" />
                    </button>
                  </div>
                  <div className="shrink-0 flex items-center gap-1">
                    {row.status === 'paid' && (
                      <Tag
                        bordered={false}
                        className="m-0 text-[11px] bg-zinc-900 text-zinc-100 dark:bg-zinc-100 dark:text-zinc-900 px-1.5 py-0.5"
                      >
                        {en ? 'Paid' : '已付款'}
                      </Tag>
                    )}
                    {row.status === 'unpaid' && (
                      <Tag
                        bordered={false}
                        className="m-0 text-[11px] bg-amber-500/15 text-amber-700 dark:text-amber-400 border border-amber-500/30 px-1.5 py-0.5 font-semibold"
                      >
                        {en ? 'Unpaid' : '待付款'}
                      </Tag>
                    )}
                    {row.status !== 'paid' && row.status !== 'unpaid' && (
                      <Tag
                        bordered={false}
                        className="m-0 text-[11px] bg-zinc-100 dark:bg-zinc-800 text-zinc-400 border border-zinc-200 dark:border-zinc-700 px-1.5 py-0.5"
                      >
                        {en ? 'Closed' : '已关闭'}
                      </Tag>
                    )}
                  </div>
                </div>

                {/* 中部核心行：方案名称 + 支付金额 */}
                <div className="flex items-start justify-between gap-3 pt-0.5">
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-bold text-foreground leading-snug tracking-tight">
                      {row.plan_name}
                    </div>
                    <div className="flex items-center gap-2 mt-1.5 text-[11px] text-muted-foreground flex-wrap">
                      <span>{formatApiDateTime(row.created_at, 'YYYY-MM-DD HH:mm')}</span>
                      <span>·</span>
                      <Tag
                        bordered={false}
                        className="m-0 text-[10px] inline-flex items-center gap-1 px-1.5 py-0 bg-zinc-100 dark:bg-zinc-800 text-zinc-700 dark:text-zinc-300 border border-zinc-200 dark:border-zinc-700"
                      >
                        {methodInfo.icon}
                        {en ? methodInfo.labelEn : methodInfo.labelZh}
                      </Tag>
                    </div>
                  </div>
                  <div className="shrink-0 text-right">
                    <div className="text-base font-extrabold text-foreground tracking-tight">
                      {formatCreditMoney(symbol, row.price_paid)}
                    </div>
                  </div>
                </div>

                {/* 待付款提示条（含倒计时） */}
                {row.status === 'unpaid' && (
                  <div
                    className="flex items-center justify-between px-2.5 py-1.5 rounded-lg bg-amber-500/10 border border-amber-500/20 text-amber-700 dark:text-amber-400 text-xs"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <span className="text-[11px] font-medium flex items-center gap-1">
                      <ClockCircleOutlined className="text-[10px]" />
                      {en ? 'Expires in' : '支付有效期剩余'}
                    </span>
                    <PayCountdown expiresAt={row.pay_expires_at} en={en} onExpire={load} />
                  </div>
                )}

                {/* 底部操作行 */}
                <div
                  className="pt-2 border-t border-border/60 flex items-center justify-between gap-2"
                  onClick={(e) => e.stopPropagation()}
                >
                  <span className="text-[11px] text-muted-foreground flex items-center gap-0.5 hover:text-foreground">
                    <span>{en ? 'Order Details' : '订单详情'}</span>
                    <RightOutlined className="text-[9px]" />
                  </span>

                  <div className="flex items-center gap-2">
                    {row.status === 'unpaid' ? (
                      <>
                        <Button
                          size="small"
                          danger
                          type="default"
                          onClick={() => cancelOrder(row)}
                          className="text-xs h-7 px-2.5 rounded-md"
                        >
                          {en ? 'Cancel' : '取消'}
                        </Button>
                        <Button
                          type="primary"
                          size="small"
                          icon={<CreditCardOutlined />}
                          onClick={() => navigate(`/orders/${row.order_no}`)}
                          className="font-medium text-xs h-7 px-3.5 rounded-md shadow-xs"
                        >
                          {en ? 'Pay Now' : '立即支付'}
                        </Button>
                      </>
                    ) : (
                      <Button
                        type="default"
                        size="small"
                        icon={<EyeOutlined />}
                        onClick={() => navigate(`/orders/${row.order_no}`)}
                        className="text-xs h-7 px-3 rounded-md"
                      >
                        {en ? 'View' : '查看'}
                      </Button>
                    )}
                  </div>
                </div>
              </div>
            );
          })
        )}

        {/* 移动端紧凑分页 */}
        {displayRows.length > 0 && (
          <div className="pt-2 pb-1 flex justify-center">
            <Pagination
              size="small"
              simple
              current={page}
              pageSize={pageSize}
              total={statusFilter === 'all' ? total : displayRows.length}
              onChange={onChange}
              showSizeChanger={false}
            />
          </div>
        )}
      </div>

      {/* 桌面端完整表格 (Desktop Table View) */}
      <div className="hidden md:block rounded-lg border border-border bg-card shadow-2xs overflow-hidden">
        <Table
          rowKey="id"
          loading={loading}
          dataSource={displayRows}
          scroll={{ x: 880 }}
          size="small"
          pagination={listPagination({
            total: statusFilter === 'all' ? total : displayRows.length,
            current: page,
            pageSize,
            onChange,
          })}
          locale={{
            emptyText: (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description={<span className="text-xs text-muted-foreground">{en ? 'No orders found' : '暂无订单记录'}</span>}
              >
                {membershipOn && (
                  <Button type="primary" size="small" icon={<ShoppingOutlined />} onClick={() => navigate('/credits', { state: offerReturnState(`${location.pathname}${location.search}${location.hash}`) })}>
                    {en ? 'Browse Plans' : '选购方案'}
                  </Button>
                )}
              </Empty>
            ),
          }}
          columns={[
            {
              title: en ? 'Order No.' : '订单号',
              dataIndex: 'order_no',
              width: 190,
              render: (value: string) => (
                <div className="flex items-center gap-1 text-xs font-semibold text-foreground">
                  <Link to={`/orders/${value}`} className="hover:underline text-foreground">
                    {value}
                  </Link>
                  <Typography.Text copyable={{ text: value }} style={{ marginLeft: 2 }} />
                </div>
              ),
            },
            {
              title: en ? 'Created' : '下单时间',
              dataIndex: 'created_at',
              width: 140,
              render: (value: string) => (
                <span className="text-xs text-muted-foreground whitespace-nowrap">
                  {formatApiDateTime(value, 'YYYY-MM-DD HH:mm')}
                </span>
              ),
            },
            {
              title: en ? 'Name' : '方案名称',
              dataIndex: 'plan_name',
              width: 160,
              render: (plan_name: string) => (
                <span className="text-xs font-semibold text-foreground">{plan_name}</span>
              ),
            },
            {
              title: en ? 'Payment Method' : '支付方式',
              dataIndex: 'pay_method',
              width: 120,
              render: (pay_method: string, row: Order) => {
                const methodInfo = getPayMethodInfo(pay_method, row.status);
                return (
                  <Tag
                    bordered={false}
                    className="m-0 text-[11px] inline-flex items-center gap-1 px-1.5 py-0.5 bg-zinc-100 dark:bg-zinc-800 text-zinc-800 dark:text-zinc-200 border border-zinc-200 dark:border-zinc-700"
                  >
                    {methodInfo.icon}
                    {en ? methodInfo.labelEn : methodInfo.labelZh}
                  </Tag>
                );
              },
            },
            {
              title: en ? 'Amount' : '支付金额',
              dataIndex: 'price_paid',
              width: 120,
              render: (value: number) => (
                <span className="text-xs font-bold text-foreground">
                  {formatCreditMoney(symbol, value)}
                </span>
              ),
            },
            {
              title: en ? 'Status' : '状态',
              dataIndex: 'status',
              width: 140,
              render: (status: string, row: Order) => {
                if (status === 'paid') {
                  return (
                    <Tag
                      bordered={false}
                      className="m-0 text-[11px] bg-zinc-900 text-zinc-100 dark:bg-zinc-100 dark:text-zinc-900"
                    >
                      {en ? 'Paid' : '已付款'}
                    </Tag>
                  );
                }
                if (status === 'unpaid') {
                  return (
                    <div className="flex flex-col gap-1 items-start">
                      <Tag
                        bordered={false}
                        className="m-0 text-[11px] bg-zinc-200 dark:bg-zinc-800 text-zinc-900 dark:text-zinc-100 border border-zinc-300 dark:border-zinc-700 font-semibold"
                      >
                        {en ? 'Unpaid' : '待付款'}
                      </Tag>
                      <PayCountdown expiresAt={row.pay_expires_at} en={en} onExpire={load} />
                    </div>
                  );
                }
                return (
                  <Tag
                    bordered={false}
                    className="m-0 text-[11px] bg-zinc-100 dark:bg-zinc-800 text-zinc-400 border border-zinc-200 dark:border-zinc-700"
                  >
                    {en ? 'Closed' : '已关闭'}
                  </Tag>
                );
              },
            },
            {
              title: en ? 'Actions' : '操作',
              key: 'actions',
              width: 130,
              align: 'center',
              render: (_: unknown, row: Order) => (
                <div className="flex items-center justify-center gap-1.5">
                  {row.status === 'unpaid' ? (
                    <>
                      <Button
                        type="primary"
                        size="small"
                        icon={<CreditCardOutlined />}
                        onClick={() => navigate(`/orders/${row.order_no}`)}
                        className="font-medium text-xs h-7"
                      >
                        {en ? 'Pay' : '去支付'}
                      </Button>
                      <Button
                        type="text"
                        size="small"
                        onClick={() => cancelOrder(row)}
                        className="text-xs h-7 text-muted-foreground hover:text-foreground"
                      >
                        {en ? 'Cancel' : '取消'}
                      </Button>
                    </>
                  ) : (
                    <Button
                      type="default"
                      size="small"
                      icon={<EyeOutlined />}
                      onClick={() => navigate(`/orders/${row.order_no}`)}
                      className="text-xs h-7"
                    >
                      {en ? 'Details' : '详情'}
                    </Button>
                  )}
                </div>
              ),
            },
          ]}
        />
      </div>
    </div>
  );
};

export default MyOrders;
