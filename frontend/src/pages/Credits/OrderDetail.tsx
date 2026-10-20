/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useCallback, useEffect, useState } from 'react';
import { Button, Modal, Spin, message } from 'antd';
import {
  ArrowLeftOutlined,
  CheckCircleFilled,
  ClockCircleOutlined,
  CloseCircleFilled,
  CopyOutlined,
  CreditCardOutlined,
  CrownOutlined,
  DatabaseOutlined,
  SafetyCertificateOutlined,
  ShoppingOutlined,
  SwapOutlined,
  ThunderboltOutlined,
  WalletOutlined,
} from '@ant-design/icons';
import { useLocation, useNavigate, useParams, useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import request from '../../utils/request';
import useSettingsStore from '../../store/settings';
import { formatApiDateTime } from '../../utils/timedisplay';
import RechargeModal from '../Wallet/RechargeModal';
import { PayCountdown } from './MyOrders';
import { formatCreditMoney } from './money';
import PageHeader from '../../components/PageHeader';
import './credits.css';

type Detail = {
  id: number;
  order_no: string;
  plan_name: string;
  price: number;
  status: string;
  pay_method: string;
  pay_expires_at: string | null;
  paid_at: string | null;
  created_at: string;
  tier_note: string;
  tier_enabled: number;
  target_group_key: string;
  tier_duration_mode: string;
  tier_duration_days: number;
  tier_duration_hours: number;
  credit_enabled: number;
  credit_amount: number;
  credit_worth?: number;
  credit_discount?: number;
  credit_model_discounts?: Record<string, number>;
  worth_value?: number | null;
  credit_duration_mode: string;
  credit_duration_days: number;
  credit_duration_hours: number;
  model_ids: string[];
  space_enabled: number;
  space_mb: number;
  space_duration_mode: string;
  space_duration_days: number;
  space_duration_hours: number;
  gift_space_mb?: number;
  gift_space_duration_mode?: string;
  gift_space_duration_days?: number;
  gift_space_duration_hours?: number;
  features: string[];
  subtitle: string;
  description: string;
  balance: number;
  balance_hold: number;
  balance_due: number;
  can_balance: boolean;
  can_combo: boolean;
  combo_balance: number;
  combo_online: number;
  face_value: number | null;
  credit_remaining: number | null;
  offer_kind?: string;
  billing_cycle?: string;
  can_renew?: boolean;
  price_month?: number | null;
  price_quarter?: number | null;
  price_year?: number | null;
  list_price?: number | null;
  list_price_month?: number | null;
  list_price_quarter?: number | null;
  list_price_year?: number | null;
  renew_price_month?: number | null;
  renew_price_quarter?: number | null;
  renew_price_year?: number | null;
  content_rev?: number;
  content_changed?: boolean;
  changes?: { key: string; before: string; after: string; money?: boolean }[];
  sale_closed?: boolean;
  cycle_closed?: boolean;
  benefit_locked?: boolean;
};

function offeredCycles(row: {
  can_renew?: boolean;
  price_month?: number | null;
  price_quarter?: number | null;
  price_year?: number | null;
  list_price_month?: number | null;
  list_price_quarter?: number | null;
  list_price_year?: number | null;
  renew_price_month?: number | null;
  renew_price_quarter?: number | null;
  renew_price_year?: number | null;
}) {
  const renewing = !!row.can_renew;
  const items: { cycle: string; price: number; list: number | null }[] = [];
  const push = (cycle: string, sale?: number | null, renew?: number | null, list?: number | null) => {
    if (sale == null) return;
    const price = renewing && renew != null ? Number(renew) : Number(sale);
    items.push({ cycle, price, list: list != null && Number(list) > price ? Number(list) : null });
  };
  push('month', row.price_month, row.renew_price_month, row.list_price_month);
  push('quarter', row.price_quarter, row.renew_price_quarter, row.list_price_quarter);
  push('year', row.price_year, row.renew_price_year, row.list_price_year);
  return items;
}

const CYCLE_RANK: Record<string, number> = {
  month: 1,
  quarter: 2,
  year: 3,
};

function visibleOfferedCycles(row: Parameters<typeof offeredCycles>[0], requestedCycle?: string | null) {
  const all = offeredCycles(row);
  if (!requestedCycle || !all.some((item) => item.cycle === requestedCycle)) {
    return all;
  }
  const minRank = CYCLE_RANK[requestedCycle] || 1;
  return all.filter((item) => (CYCLE_RANK[item.cycle] || 0) >= minRank);
}

function cycleTitle(cycle: string | undefined, en: boolean) {
  if (cycle === 'quarter') return en ? 'quarter' : '季';
  if (cycle === 'year') return en ? 'year' : '年';
  return en ? 'month' : '月';
}

type Choice = 'balance' | 'online' | 'combo';

const STATUS: Record<string, [string, string]> = {
  draft: ['待确认支付', 'Ready to Pay'],
  unpaid: ['待支付', 'Unpaid'],
  paid: ['已付款', 'Paid'],
  closed: ['已关闭', 'Closed'],
};

function quoteFrom(price: number, balance: number, hold = 0) {
  const balanceDue = Math.max(0, price - hold);
  const canBalance = balance + 0.0000001 >= balanceDue;
  const comboOnline = hold > 0 ? balanceDue : Math.max(0, price - balance);
  const comboBalance = hold > 0 ? hold : Math.max(0, price - comboOnline);
  return {
    balance_due: balanceDue,
    can_balance: canBalance,
    can_combo: comboBalance > 0 && comboOnline >= 0.01 && !canBalance,
    combo_balance: comboBalance,
    combo_online: comboOnline,
  };
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

const OrderDetail: React.FC = () => {
  const { orderNo = '', planId = '' } = useParams();
  const preview = Boolean(planId);
  const location = useLocation();
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const { i18n } = useTranslation();
  const en = (i18n.language || '').toLowerCase().startsWith('en');
  const { settings } = useSettingsStore();
  const symbol = settings?.currency?.currency_symbol || '$';
  const [activeOrderNo, setActiveOrderNo] = useState(orderNo);
  const [order, setOrder] = useState<Detail | null>(null);
  const [blocked, setBlocked] = useState('');
  const [loading, setLoading] = useState(true);
  const [choice, setChoice] = useState<Choice>('online');
  const [paying, setPaying] = useState(false);
  const [switchingCycle, setSwitchingCycle] = useState(false);
  const [onlineOpen, setOnlineOpen] = useState(false);
  const [onlineAmount, setOnlineAmount] = useState(0);
  const initializedRef = React.useRef(false);

  useEffect(() => {
    if (orderNo) setActiveOrderNo(orderNo);
  }, [orderNo]);

  const applyChoice = (res: Pick<Detail, 'can_balance' | 'can_combo' | 'combo_online' | 'price'>) => {
    setChoice(res.can_balance ? 'balance' : res.can_combo ? 'combo' : 'online');
    setOnlineAmount(res.can_combo ? res.combo_online : res.price);
  };

  const fetchOrderByNo = useCallback(async (targetNo: string) => {
    const res = (await request.get(`/plugins/marketing_membership/my-orders/${encodeURIComponent(targetNo)}`)) as Detail;
    setActiveOrderNo(res.order_no);
    setOrder((prev) => (prev ? { ...prev, ...res } : res));
    applyChoice(res);
    return res;
  }, []);

  const load = useCallback(async (targetNo?: string) => {
    const isInitial = !initializedRef.current;
    const minWait = isInitial ? sleep(420) : Promise.resolve();
    setLoading(true);
    try {
      const currentOrderNo = targetNo || activeOrderNo || orderNo;
      if (currentOrderNo) {
        const res = (await request.get(`/plugins/marketing_membership/my-orders/${encodeURIComponent(currentOrderNo)}`)) as Detail;
        await minWait;
        setActiveOrderNo(res.order_no);
        setOrder((prev) => (prev ? { ...prev, ...res } : res));
        applyChoice(res);
        return;
      }
      if (preview) {
        const previewRes = (await request.get(`/plugins/marketing_membership/checkout/${planId}`)) as {
          state: string;
          message?: string;
          plan: (Detail & { id: number; name: string; price: number; action?: string; content_rev?: number }) | null;
          balance: number;
          content_changed?: boolean;
          changes?: Detail['changes'];
          content_rev?: number;
          renew_cycle?: string;
        };
        await minWait;
        const plan = previewRes.plan;
        if (!plan || previewRes.state !== 'open' || plan.action !== 'buy') {
          setOrder(null);
          setBlocked(previewRes.message || (en ? 'This plan cannot be purchased.' : '这个方案现在不能购买。'));
          return;
        }
        setBlocked('');
        const cycles = plan.offer_kind === 'subscription' ? offeredCycles(plan) : [];
        const preferredCycle = searchParams.get('cycle') || previewRes.renew_cycle || '';
        const chosen = cycles.find((item) => item.cycle === preferredCycle) || cycles[0];
        const price = chosen ? chosen.price : Number(plan.price);
        const listPrice = chosen ? chosen.list : (plan.list_price ?? null);
        const quote = quoteFrom(price, Number(previewRes.balance || 0));
        setOrder({
          ...quote,
          id: 0,
          order_no: '',
          plan_name: plan.name,
          price,
          list_price: listPrice,
          status: 'draft',
          pay_method: '',
          pay_expires_at: null,
          paid_at: null,
          created_at: '',
          tier_note: '',
          tier_enabled: plan.tier_enabled,
          target_group_key: plan.target_group_key,
          tier_duration_mode: plan.tier_duration_mode,
          tier_duration_days: plan.tier_duration_days,
          tier_duration_hours: plan.tier_duration_hours,
          credit_enabled: plan.credit_enabled,
          credit_amount: plan.credit_amount,
          credit_worth: plan.credit_worth,
          credit_discount: plan.credit_discount,
          credit_model_discounts: plan.credit_model_discounts,
          credit_duration_mode: plan.credit_duration_mode,
          credit_duration_days: plan.credit_duration_days,
          credit_duration_hours: plan.credit_duration_hours,
          model_ids: plan.model_ids || [],
          space_enabled: plan.space_enabled,
          space_mb: plan.space_mb,
          space_duration_mode: plan.space_duration_mode,
          space_duration_days: plan.space_duration_days,
          space_duration_hours: plan.space_duration_hours,
          gift_space_mb: plan.gift_space_mb,
          gift_space_duration_mode: plan.gift_space_duration_mode,
          gift_space_duration_days: plan.gift_space_duration_days,
          gift_space_duration_hours: plan.gift_space_duration_hours,
          features: plan.features || [],
          subtitle: plan.subtitle,
          description: plan.description,
          offer_kind: plan.offer_kind,
          billing_cycle: chosen ? chosen.cycle : '',
          can_renew: plan.can_renew,
          price_month: plan.price_month,
          price_quarter: plan.price_quarter,
          price_year: plan.price_year,
          list_price_month: plan.list_price_month,
          list_price_quarter: plan.list_price_quarter,
          list_price_year: plan.list_price_year,
          renew_price_month: plan.renew_price_month,
          renew_price_quarter: plan.renew_price_quarter,
          renew_price_year: plan.renew_price_year,
          balance: Number(previewRes.balance || 0),
          balance_hold: 0,
          face_value: null,
          credit_remaining: null,
          content_rev: previewRes.content_rev || plan.content_rev,
          content_changed: previewRes.content_changed,
          changes: previewRes.changes || [],
        });
        applyChoice({ ...quote, price });
      }
    } finally {
      await minWait;
      initializedRef.current = true;
      setLoading(false);
    }
  }, [activeOrderNo, en, orderNo, planId, preview, searchParams]);

  useEffect(() => {
    void Promise.resolve().then(() => {
      load();
    });
  }, [load]);

  useEffect(() => {
    const payment = searchParams.get('payment');
    if (payment !== 'success' && payment !== 'cancelled') return;
    if (payment === 'success') message.success(en ? 'Payment received' : '支付结果已更新');
    if (payment === 'cancelled') message.info(en ? 'Payment was not finished' : '在线支付未完成，订单仍可继续支付');
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      next.delete('payment');
      return next;
    }, { replace: true });
    void Promise.resolve().then(() => {
      load();
    });
  }, [en, load, searchParams, setSearchParams]);

  const duration = (mode: string, days: number, hours = 0, permanent = en ? 'no expiry' : '长期') => {
    if (mode === 'cycle') {
      if (order?.billing_cycle === 'quarter') return en ? '90 days' : '90 天';
      if (order?.billing_cycle === 'year') return en ? '365 days' : '365 天';
      return en ? '30 days' : '30 天';
    }
    if (mode === 'hours') return en ? `${hours || 24} hours` : `${hours || 24}小时`;
    if (mode === 'days') return en ? `${days || 30} days` : `${days || 30}天`;
    if (days && days > 0) return en ? `${days} days` : `${days}天`;
    if (hours && hours > 0) return en ? `${hours} hours` : `${hours}小时`;
    return permanent;
  };

  const selectableCycles = order ? visibleOfferedCycles(order, searchParams.get('cycle')) : [];

  const canSwitchCycle = Boolean(
    order &&
      order.offer_kind === 'subscription' &&
      selectableCycles.length > 1 &&
      (order.status === 'draft' || (order.status === 'unpaid' && !order.benefit_locked && order.balance_hold <= 0)),
  );

  const selectCycle = async (cycle: string) => {
    if (!order || !canSwitchCycle || order.billing_cycle === cycle || switchingCycle || paying) return;
    const found = selectableCycles.find((item) => item.cycle === cycle);
    if (!found) return;
    setSwitchingCycle(true);
    await sleep(260);
    const quote = quoteFrom(found.price, order.balance, order.balance_hold);
    setOrder((prev) => (prev ? { ...prev, ...quote, price: found.price, list_price: found.list, billing_cycle: cycle } : prev));
    applyChoice({ ...quote, price: found.price });
    setSwitchingCycle(false);
  };

  const pay = async () => {
    if (!order || (order.status !== 'unpaid' && order.status !== 'draft')) return;
    setPaying(true);
    const minWait = sleep(650);
    let currentNo = order.order_no || activeOrderNo;
    try {
      let targetId = order.id;
      if (order.status === 'draft' || (preview && planId && !order.benefit_locked && order.balance_hold <= 0)) {
        const created = (await request.post('/plugins/marketing_membership/purchase', {
          plan_id: Number(planId),
          billing_cycle: order.offer_kind === 'subscription' ? order.billing_cycle : '',
          content_rev: order.content_rev || 0,
        })) as {
          order_id?: number;
          order_no?: string;
          status?: string;
        };
        if (!created.order_no || !created.order_id) {
          await minWait;
          return;
        }
        targetId = created.order_id;
        currentNo = created.order_no;
        setActiveOrderNo(created.order_no);
        if (created.status === 'paid') {
          const res = (await request.get(`/plugins/marketing_membership/my-orders/${encodeURIComponent(created.order_no)}`)) as Detail;
          await minWait;
          setOrder((prev) => (prev ? { ...prev, ...res } : res));
          applyChoice(res);
          message.success(en ? 'Paid. The coupon is in your account.' : '支付成功，卡券已发放');
          return;
        }
      }
      if (choice === 'balance') {
        await request.post(`/plugins/marketing_membership/orders/${targetId}/pay`);
        const res = currentNo
          ? ((await request.get(`/plugins/marketing_membership/my-orders/${encodeURIComponent(currentNo)}`)) as Detail)
          : null;
        await minWait;
        if (res) {
          setOrder((prev) => (prev ? { ...prev, ...res } : res));
          applyChoice(res);
        }
        message.success(en ? 'Paid. The coupon is in your account.' : '支付成功，卡券已发放');
        return;
      }
      const prepared = (await request.post(`/plugins/marketing_membership/orders/${targetId}/pay-online`, {
        use_balance: choice === 'combo',
      })) as { online_amount: number };
      const res = currentNo
        ? ((await request.get(`/plugins/marketing_membership/my-orders/${encodeURIComponent(currentNo)}`)) as Detail)
        : null;
      await minWait;
      setOnlineAmount(Number(prepared.online_amount || 0));
      if (res) {
        setOrder((prev) => (prev ? { ...prev, ...res } : res));
        applyChoice(res);
      }
      setOnlineOpen(true);
    } catch (err: unknown) {
      await minWait;
      const data = (err as { response?: { data?: { message?: string; error?: { message?: string } } } })?.response?.data;
      const serverMsg = data?.error?.message || data?.message || '';
      if (serverMsg.includes('方案内容已更新') || serverMsg.includes('已下架') || serverMsg.includes('已停售')) {
        message.warning(serverMsg);
      }
      await load(currentNo || undefined);
    } finally {
      setPaying(false);
    }
  };

  const cancel = () => {
    if (!order || !order.id) return;
    Modal.confirm({
      title: en ? 'Cancel this unpaid order?' : '取消这笔未付款订单？',
      content: en ? 'Reserved balance will be returned.' : '已占用的余额会退回。',
      okText: en ? 'Cancel order' : '确认取消',
      cancelText: en ? 'Back' : '返回',
      onOk: async () => {
        const minWait = sleep(500);
        await request.post(`/plugins/marketing_membership/orders/${order.id}/cancel`);
        await minWait;
        await fetchOrderByNo(order.order_no);
        message.success(en ? 'Order closed' : '订单已关闭');
      },
    });
  };

  const leave = () => {
    if (location.key !== 'default') {
      navigate(-1);
      return;
    }
    navigate(preview ? '/credits' : '/orders');
  };

  if (loading && !order) {
    return (
      <div className="order-standalone order-checkout-wrapper flex flex-col items-center justify-center min-h-[50vh] gap-3">
        <Spin size="large" />
        <div className="text-xs text-zinc-500 dark:text-zinc-400">
          {en ? 'Loading order details...' : '正在加载订单信息...'}
        </div>
      </div>
    );
  }

  if (!order) {
    return (
      <div className="order-standalone order-checkout-wrapper">
        <Button className="order-standalone-back" type="text" icon={<ArrowLeftOutlined />} onClick={leave}>
          {en ? 'Back' : '返回'}
        </Button>
        <div className="order-checkout-page">
          <div className="bg-white dark:bg-[#141414] border border-zinc-200 dark:border-zinc-800 rounded-xl p-6 text-center max-w-sm mx-auto shadow-2xs">
            <div className="w-10 h-10 rounded-lg bg-zinc-100 dark:bg-zinc-900 border border-zinc-200 dark:border-zinc-800 text-zinc-500 dark:text-zinc-400 flex items-center justify-center mx-auto mb-3 text-base">
              <ShoppingOutlined />
            </div>
            <div className="text-base font-semibold text-zinc-950 dark:text-zinc-50 mb-1.5">
              {preview ? (en ? 'Plan Unavailable' : '方案不可购买') : (en ? 'Order Not Found' : '未找到相关订单')}
            </div>
            <p className="text-xs text-zinc-500 dark:text-zinc-400 mb-4">
              {blocked || (preview ? (en ? 'This plan cannot be purchased.' : '这个方案现在不能购买。') : (en ? 'This order was not found.' : '没有找到这笔订单。'))}
            </p>
            <Button type="primary" size="small" onClick={() => navigate(preview ? '/credits' : '/orders')} className="!rounded-md">
              {preview ? (en ? 'Explore other plans' : '浏览其他卡券方案') : (en ? 'View all orders' : '查看我的订单')}
            </Button>
          </div>
        </div>
      </div>
    );
  }

  const status = STATUS[order.status || 'draft'] || STATUS.draft;
  const shortfall = Math.max(0, order.balance_due - order.balance);
  const discountAmount = order.list_price != null && order.list_price > order.price ? order.list_price - order.price : 0;

  return (
    <div className="order-standalone order-checkout-wrapper">
      <Button className="order-standalone-back" type="text" icon={<ArrowLeftOutlined />} onClick={leave}>
        {en ? 'Back' : '返回'}
      </Button>
      <div className="order-checkout-page">

        {/* Page Title & Status Header (统一合并确认订单与订单详情) */}
        <PageHeader
          title={
            <span className="inline-flex items-center gap-2.5 flex-wrap">
              <span>{en ? 'Confirm Order' : '确认订单'}</span>
              <span
                className={`px-2 py-0.5 rounded-md text-xs font-medium tracking-normal ${
                  order.status === 'paid'
                    ? 'bg-zinc-900 text-zinc-50 dark:bg-zinc-100 dark:text-zinc-900'
                    : order.status === 'closed'
                    ? 'bg-zinc-100 dark:bg-zinc-900 text-zinc-500 dark:text-zinc-400 border border-zinc-200 dark:border-zinc-800'
                    : 'bg-zinc-100 dark:bg-zinc-800 text-zinc-700 dark:text-zinc-300 border border-zinc-200 dark:border-zinc-700'
                }`}
              >
                {status[en ? 1 : 0]}
              </span>
            </span>
          }
          description={
            order.order_no ? (
              <span className="inline-flex flex-wrap items-center gap-1.5 text-xs text-zinc-500 dark:text-zinc-400">
                <span>{en ? 'Order No:' : '订单号：'}</span>
                <span className="text-zinc-700 dark:text-zinc-300">{order.order_no}</span>
                <button
                  type="button"
                  onClick={() => {
                    navigator.clipboard.writeText(order.order_no);
                    message.success(en ? 'Copied order number' : '已复制订单号');
                  }}
                  className="text-zinc-400 hover:text-zinc-900 dark:hover:text-zinc-100 transition-colors cursor-pointer"
                  title={en ? 'Copy' : '复制'}
                >
                  <CopyOutlined />
                </button>
                {(order.paid_at || order.created_at) && (
                  <>
                    <span>·</span>
                    <span>{formatApiDateTime(order.paid_at || order.created_at, 'YYYY-MM-DD HH:mm')}</span>
                  </>
                )}
              </span>
            ) : (
              en
                ? 'Confirm plan privileges and payment method. Benefits take effect immediately upon payment.'
                : '确认方案权益与支付方式，支付成功后立即生效。'
            )
          }
          extra={
            order.status === 'unpaid' ? (
              <PayCountdown expiresAt={order.pay_expires_at} en={en} onExpire={() => load(order.order_no)} />
            ) : undefined
          }
          marginBottom={16}
        />

        {/* Stacked Top-and-Bottom Structure */}
        <div className="flex flex-col gap-3.5">
          {/* Top Card: Product Showcase Card */}
          <div className="bg-white dark:bg-[#141414] border border-zinc-200 dark:border-zinc-800 rounded-xl p-4 sm:p-5 shadow-2xs relative overflow-hidden">
            {/* Product Header */}
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-zinc-100 dark:bg-[#1c1c1f] border border-zinc-200 dark:border-zinc-800 text-zinc-900 dark:text-zinc-100 flex items-center justify-center shrink-0 text-base shadow-2xs">
                {order.space_enabled === 1 && !order.tier_enabled ? (
                  <DatabaseOutlined />
                ) : order.tier_enabled === 1 ? (
                  <CrownOutlined />
                ) : (
                  <ShoppingOutlined />
                )}
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2 flex-wrap">
                  <div className="text-[16px] font-semibold text-zinc-950 dark:text-zinc-50 tracking-tight m-0">
                    {order.plan_name}
                  </div>
                  <span className="px-2 py-0.5 rounded-md text-[11px] font-medium bg-zinc-100 dark:bg-[#1c1c1f] text-zinc-600 dark:text-zinc-300 border border-zinc-200 dark:border-zinc-800">
                    {order.offer_kind === 'subscription' ? (en ? 'Subscription' : '周期订阅') : (en ? 'One-time Package' : '特惠资源包')}
                  </span>
                </div>
              </div>
            </div>

            {/* Cycle Selector (if switchable subscription) */}
            {canSwitchCycle && (
              <div className="mt-3.5">
                <div className="text-xs font-medium text-zinc-500 dark:text-zinc-400 mb-2">
                  {en ? 'Select Billing Cycle' : '选择订购周期'}
                </div>
                <div className={`grid ${selectableCycles.length === 2 ? 'grid-cols-2' : 'grid-cols-3'} gap-2`}>
                  {selectableCycles.map((item) => {
                    const active = order.billing_cycle === item.cycle;
                    return (
                      <button
                        key={item.cycle}
                        type="button"
                        onClick={() => selectCycle(item.cycle)}
                        className={`py-2 px-3 rounded-lg border text-center transition-all cursor-pointer ${
                          active
                            ? 'bg-zinc-900 text-zinc-50 border-zinc-900 dark:bg-zinc-100 dark:text-zinc-900 dark:border-zinc-100 font-semibold shadow-2xs'
                            : 'bg-zinc-50/70 dark:bg-[#1c1c1f] border-zinc-200 dark:border-zinc-800 text-zinc-700 dark:text-zinc-300 hover:border-zinc-300 dark:hover:border-zinc-700'
                        }`}
                      >
                        <div className="text-[11px] font-medium opacity-85">
                          {en ? cycleTitle(item.cycle, true) : `按${cycleTitle(item.cycle, false)}`}
                        </div>
                        <div className="text-xs font-bold mt-0.5">
                          {formatCreditMoney(symbol, item.price)}
                        </div>
                      </button>
                    );
                  })}
                </div>
              </div>
            )}

            {/* Price Showcase Block */}
            <div className="mt-3.5 p-3.5 rounded-lg bg-zinc-50 dark:bg-[#1c1c1f] border border-zinc-200/80 dark:border-zinc-800 flex items-baseline flex-wrap gap-2">
              {discountAmount > 0 && (
                <>
                  <span className="text-sm line-through text-zinc-400 dark:text-zinc-500 font-medium">
                    {formatCreditMoney(symbol, order.list_price ?? 0)}
                  </span>
                  <span className="text-[10px] font-semibold px-1.5 py-0.5 rounded bg-zinc-900 text-zinc-50 dark:bg-zinc-100 dark:text-zinc-900">
                    {en ? `Save ${formatCreditMoney(symbol, discountAmount)}` : `立省 ${formatCreditMoney(symbol, discountAmount)}`}
                  </span>
                </>
              )}
              <span className="text-[24px] font-bold tracking-tight text-zinc-950 dark:text-zinc-50 leading-none">
                {formatCreditMoney(symbol, order.price)}
              </span>
              <span className="text-xs font-medium text-zinc-500 dark:text-zinc-400">
                {order.offer_kind === 'subscription'
                  ? `/ ${cycleTitle(order.billing_cycle, en)}${order.can_renew ? (en ? ' · renew' : ' · 续费') : ''}`
                  : (en ? '/ once' : '/ 一次性生效')}
              </span>
            </div>

            {/* Privileges & Details */}
            <div className="mt-3.5">
              <div className="text-xs font-medium text-zinc-500 dark:text-zinc-400 mb-2 flex items-center gap-1.5">
                <span className="w-1 h-3 rounded-full bg-zinc-900 dark:bg-zinc-100" />
                {en ? 'Included Privileges & Highlights' : '方案所含专享权益与明细'}
              </div>
              <div className="space-y-1.5">
                {/* Storage Space */}
                {order.space_enabled === 1 && (
                  <div className="px-3 py-2 rounded-lg bg-zinc-50/70 dark:bg-[#1c1c1f]/70 border border-zinc-200/80 dark:border-zinc-800 flex items-center gap-2">
                    <DatabaseOutlined className="text-zinc-500 dark:text-zinc-400 text-xs shrink-0" />
                    <div className="min-w-0 flex-1 flex flex-col gap-1">
                      <div className="text-xs font-medium text-zinc-900 dark:text-zinc-100">
                        {en ? `Playground Storage +${order.space_mb} MB` : `创作空间存储扩容 +${order.space_mb} MB`}
                      </div>
                      <div className="text-[11px] text-zinc-500 dark:text-zinc-400">
                        {en ? 'Validity: ' : '服务有效期：'}
                        <span className="font-medium text-zinc-800 dark:text-zinc-200">
                          {order.offer_kind === 'subscription'
                            ? (en ? 'with subscription' : '随订阅')
                            : duration(order.space_duration_mode || 'permanent', order.space_duration_days || 0, order.space_duration_hours || 0, en ? 'permanent' : '永久有效')}
                        </span>
                      </div>
                    </div>
                  </div>
                )}
                {(order.gift_space_mb || 0) > 0 && (
                  <div className="px-3 py-2 rounded-lg bg-zinc-50/70 dark:bg-[#1c1c1f]/70 border border-zinc-200/80 dark:border-zinc-800 flex items-center gap-2">
                    <DatabaseOutlined className="text-zinc-500 dark:text-zinc-400 text-xs shrink-0" />
                    <div className="min-w-0 flex-1 flex flex-col gap-1">
                      <div className="text-xs font-medium text-zinc-900 dark:text-zinc-100">
                        {en ? `Gift storage +${order.gift_space_mb} MB` : `赠送扩容空间 +${order.gift_space_mb} MB`}
                      </div>
                      <div className="text-[11px] text-zinc-500 dark:text-zinc-400">
                        {en ? 'Validity: ' : '服务有效期：'}
                        <span className="font-medium text-zinc-800 dark:text-zinc-200">
                          {order.offer_kind === 'subscription'
                            ? (en ? 'with subscription' : '随订阅')
                            : duration(order.gift_space_duration_mode || 'permanent', order.gift_space_duration_days || 0, order.gift_space_duration_hours || 0, en ? 'permanent' : '永久有效')}
                        </span>
                      </div>
                    </div>
                  </div>
                )}
                {/* Membership Tier */}
                {order.tier_enabled === 1 && (
                  <div className="px-3 py-2 rounded-lg bg-zinc-50/70 dark:bg-[#1c1c1f]/70 border border-zinc-200/80 dark:border-zinc-800 flex items-center gap-2">
                    <CrownOutlined className="text-zinc-500 dark:text-zinc-400 text-xs shrink-0" />
                    <div className="min-w-0 flex-1 flex flex-col gap-1">
                      <div className="text-xs font-medium text-zinc-900 dark:text-zinc-100">
                        {en ? `Membership Tier: ${order.target_group_key}` : `专享会员等级升级：${order.target_group_key}`}
                      </div>
                      <div className="text-[11px] text-zinc-500 dark:text-zinc-400">
                        {en ? 'Special group permissions · ' : '享受等级专属费率 · '}
                        <span className="font-medium text-zinc-800 dark:text-zinc-200">
                          {duration(order.tier_duration_mode, order.tier_duration_days, order.tier_duration_hours)}
                        </span>
                      </div>
                    </div>
                  </div>
                )}
                {/* Token Credits */}
                {order.credit_enabled === 1 && (
                  <div className="px-3 py-2 rounded-lg bg-zinc-50/70 dark:bg-[#1c1c1f]/70 border border-zinc-200/80 dark:border-zinc-800 flex items-center gap-2">
                    <ThunderboltOutlined className="text-zinc-500 dark:text-zinc-400 text-xs shrink-0" />
                    <div className="min-w-0 flex-1 flex flex-col gap-1">
                      <div className="text-xs font-medium text-zinc-900 dark:text-zinc-100 flex items-center gap-1.5 flex-wrap">
                        <span>{en ? 'Face Value Credit: ' : '专享消费额度：'}{formatCreditMoney(symbol, order.credit_amount)}</span>
                        {Number(order.credit_worth || order.worth_value || 0) > order.credit_amount && (
                          <span className="text-[11px] font-normal text-zinc-500 dark:text-zinc-400">
                            {en
                              ? `(Worth ${formatCreditMoney(symbol, Number(order.credit_worth || order.worth_value || 0))})`
                              : `（面额价值 ${formatCreditMoney(symbol, Number(order.credit_worth || order.worth_value || 0))}）`}
                          </span>
                        )}
                      </div>
                      <div className="text-[11px] text-zinc-500 dark:text-zinc-400">
                        {order.credit_discount != null && order.credit_discount > 0 && order.credit_discount < 1
                          ? (en
                            ? `Coupon rate ${Math.round(order.credit_discount * 100)}% · `
                            : `卡券折扣 ${Number((order.credit_discount * 10).toFixed(2))}折 · `)
                          : Object.keys(order.credit_model_discounts || {}).length > 0
                            ? (en ? 'Model discount · ' : '模型专属折扣 · ')
                            : ''}
                        {order.model_ids?.length
                          ? (en ? `Models: ${order.model_ids.join(', ')} · ` : `指定模型：${order.model_ids.join(', ')} · `)
                          : (en ? 'All models supported · ' : '全平台所有可用模型通用 · ')}
                        <span className="font-medium text-zinc-800 dark:text-zinc-200">
                          {duration(order.credit_duration_mode, order.credit_duration_days, order.credit_duration_hours)}
                        </span>
                      </div>
                    </div>
                  </div>
                )}
                {/* Features List */}
                {order.features.map((line) => (
                  <div key={line} className="px-3 py-2 rounded-lg bg-zinc-50/70 dark:bg-[#1c1c1f]/70 border border-zinc-200/80 dark:border-zinc-800 flex items-center gap-2">
                    <CheckCircleFilled className="text-zinc-500 dark:text-zinc-400 text-xs shrink-0" />
                    <span className="text-xs text-zinc-700 dark:text-zinc-300">{line}</span>
                  </div>
                ))}
              </div>
            </div>
          </div>

          {/* Bottom Card: Payment & Settlement Status Card */}
          <div className="bg-white dark:bg-[#141414] border border-zinc-200 dark:border-zinc-800 rounded-xl p-4 sm:p-5 shadow-2xs relative overflow-hidden">
            <div className="flex items-center justify-between gap-2 pb-3 border-b border-zinc-200/80 dark:border-zinc-800">
              <div className="flex items-center gap-2 flex-wrap">
                <div className="text-sm sm:text-base font-semibold text-zinc-950 dark:text-zinc-50 flex items-center gap-2 m-0">
                  <WalletOutlined className="text-zinc-700 dark:text-zinc-300" />
                  <span>{en ? 'Settlement & Payment' : '结算与支付方式'}</span>
                </div>
                <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] font-medium text-zinc-600 dark:text-zinc-400 bg-zinc-100 dark:bg-[#1c1c1f] border border-zinc-200 dark:border-zinc-800">
                  <SafetyCertificateOutlined className="text-zinc-500 dark:text-zinc-400 text-xs" />
                  <span>{en ? 'Official Secure Checkout' : '官方安全支付保障'}</span>
                </span>
              </div>
              <span className="text-[10px] font-semibold text-zinc-400 dark:text-zinc-500 tracking-wider uppercase shrink-0">
                {order.status === 'paid' ? 'PAID' : order.status === 'closed' ? 'CLOSED' : 'CHECKOUT'}
              </span>
            </div>

            {(order.status === 'unpaid' || order.status === 'draft') && (
              <Spin spinning={paying || switchingCycle}>
                {/* Payment Methods Choice */}
                <div className="mt-3.5">
                  <div className="text-xs font-medium text-zinc-500 dark:text-zinc-400 mb-2">
                    {en ? 'Choose Payment Method' : '选择支付方式'}
                  </div>
                  <div className="space-y-2">
                    {/* Option 1: Balance Payment */}
                    <div
                      onClick={() => {
                        if (order.can_balance && !paying) setChoice('balance');
                      }}
                      className={`p-3 rounded-lg border transition-all flex items-center justify-between gap-3 ${
                        !order.can_balance
                          ? 'opacity-55 cursor-not-allowed bg-zinc-50/40 dark:bg-zinc-900/20 border-zinc-200 dark:border-zinc-800'
                          : choice === 'balance'
                          ? 'cursor-pointer bg-zinc-50 dark:bg-[#1c1c1f] border-zinc-900 dark:border-zinc-100 ring-1 ring-zinc-900 dark:ring-zinc-100 shadow-2xs'
                          : 'cursor-pointer bg-white dark:bg-[#141414] border-zinc-200 dark:border-zinc-800 hover:border-zinc-300 dark:hover:border-zinc-700 hover:bg-zinc-50/50 dark:hover:bg-[#1c1c1f]/50'
                      }`}
                    >
                      <div className="flex items-center gap-3 min-w-0">
                        <div
                          className={`w-4 h-4 rounded-full border flex items-center justify-center shrink-0 transition-colors ${
                            choice === 'balance' && order.can_balance
                              ? 'border-zinc-900 bg-zinc-900 dark:border-zinc-100 dark:bg-zinc-100'
                              : 'border-zinc-300 dark:border-zinc-600'
                          }`}
                        >
                          {choice === 'balance' && order.can_balance && (
                            <div className="w-1.5 h-1.5 rounded-full bg-white dark:bg-zinc-900" />
                          )}
                        </div>
                        <div className="w-8 h-8 rounded-md bg-zinc-100 dark:bg-zinc-800 border border-zinc-200/70 dark:border-zinc-700/70 text-zinc-800 dark:text-zinc-200 flex items-center justify-center shrink-0 text-sm">
                          <WalletOutlined />
                        </div>
                        <div className="min-w-0">
                          <div className="text-xs sm:text-sm font-medium text-zinc-950 dark:text-zinc-50 flex items-center gap-1.5 flex-wrap">
                            <span>{en ? 'Pay with Balance' : '账户余额支付'}</span>
                            <span className="text-xs font-normal text-zinc-500 dark:text-zinc-400">
                              {en
                                ? `(Balance: ${formatCreditMoney(symbol, order.balance)})`
                                : `(可用余额: ${formatCreditMoney(symbol, order.balance)})`}
                            </span>
                            {order.can_balance ? (
                              <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-zinc-900 text-zinc-50 dark:bg-zinc-100 dark:text-zinc-900">
                                {en ? 'Instant' : '极速秒扣'}
                              </span>
                            ) : (
                              <span className="inline-flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded bg-zinc-100 dark:bg-zinc-800 text-zinc-600 dark:text-zinc-400 border border-zinc-200 dark:border-zinc-700">
                                <span>{en ? 'Insufficient' : '余额不足'}</span>
                                <button
                                  type="button"
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    navigate('/wallet');
                                  }}
                                  className="underline underline-offset-2 text-zinc-900 dark:text-zinc-100 hover:opacity-80 cursor-pointer ml-0.5 font-semibold"
                                >
                                  {en ? 'Recharge' : '去充值'}
                                </button>
                              </span>
                            )}
                          </div>
                          <div className="text-[11px] text-zinc-500 dark:text-zinc-400 truncate mt-0.5">
                            {en
                              ? `Deduct ${formatCreditMoney(symbol, order.balance_due)} from balance`
                              : `从余额扣除 ${formatCreditMoney(symbol, order.balance_due)}`}
                            {order.balance_hold > 0 && (
                              <span>
                                {' · '}
                                {en ? 'Reserved' : '已占用'} {formatCreditMoney(symbol, order.balance_hold)}
                              </span>
                            )}
                            {!order.can_balance && (
                              <span className="text-zinc-700 dark:text-zinc-300 font-medium">
                                {' · '}
                                {en ? `Need ${formatCreditMoney(symbol, shortfall)} more` : `还差 ${formatCreditMoney(symbol, shortfall)}`}
                              </span>
                            )}
                          </div>
                        </div>
                      </div>
                      <div className="text-right shrink-0">
                        <div className="text-xs sm:text-sm font-bold text-zinc-950 dark:text-zinc-50">
                          {formatCreditMoney(symbol, order.balance_due)}
                        </div>
                      </div>
                    </div>

                    {/* Option 2: Full Online Payment */}
                    <div
                      onClick={() => {
                        if (!paying) setChoice('online');
                      }}
                      className={`p-3 rounded-lg border transition-all cursor-pointer flex items-center justify-between gap-3 ${
                        choice === 'online'
                          ? 'bg-zinc-50 dark:bg-[#1c1c1f] border-zinc-900 dark:border-zinc-100 ring-1 ring-zinc-900 dark:ring-zinc-100 shadow-2xs'
                          : 'bg-white dark:bg-[#141414] border-zinc-200 dark:border-zinc-800 hover:border-zinc-300 dark:hover:border-zinc-700 hover:bg-zinc-50/50 dark:hover:bg-[#1c1c1f]/50'
                      }`}
                    >
                      <div className="flex items-center gap-3 min-w-0">
                        <div
                          className={`w-4 h-4 rounded-full border flex items-center justify-center shrink-0 transition-colors ${
                            choice === 'online'
                              ? 'border-zinc-900 bg-zinc-900 dark:border-zinc-100 dark:bg-zinc-100'
                              : 'border-zinc-300 dark:border-zinc-600'
                          }`}
                        >
                          {choice === 'online' && <div className="w-1.5 h-1.5 rounded-full bg-white dark:bg-zinc-900" />}
                        </div>
                        <div className="w-8 h-8 rounded-md bg-zinc-100 dark:bg-zinc-800 border border-zinc-200/70 dark:border-zinc-700/70 text-zinc-800 dark:text-zinc-200 flex items-center justify-center shrink-0 text-sm">
                          <CreditCardOutlined />
                        </div>
                        <div className="min-w-0">
                          <div className="text-xs sm:text-sm font-medium text-zinc-950 dark:text-zinc-50">
                            {en ? 'Pay Full Online' : '在线全额支付'}
                          </div>
                          <div className="text-[11px] text-zinc-500 dark:text-zinc-400 truncate mt-0.5">
                            {en ? 'WeChat, Alipay, Cards, etc.' : '支持微信、支付宝、网银等'}
                          </div>
                        </div>
                      </div>
                      <div className="text-right shrink-0">
                        <div className="text-xs sm:text-sm font-bold text-zinc-950 dark:text-zinc-50">
                          {formatCreditMoney(symbol, order.price)}
                        </div>
                      </div>
                    </div>

                    {/* Option 3: Combo Balance + Online (if available) */}
                    {order.can_combo && (
                      <div
                        onClick={() => {
                          if (!paying) setChoice('combo');
                        }}
                        className={`p-3 rounded-lg border transition-all cursor-pointer flex items-center justify-between gap-3 ${
                          choice === 'combo'
                            ? 'bg-zinc-50 dark:bg-[#1c1c1f] border-zinc-900 dark:border-zinc-100 ring-1 ring-zinc-900 dark:ring-zinc-100 shadow-2xs'
                            : 'bg-white dark:bg-[#141414] border-zinc-200 dark:border-zinc-800 hover:border-zinc-300 dark:hover:border-zinc-700 hover:bg-zinc-50/50 dark:hover:bg-[#1c1c1f]/50'
                        }`}
                      >
                        <div className="flex items-center gap-3 min-w-0">
                          <div
                            className={`w-4 h-4 rounded-full border flex items-center justify-center shrink-0 transition-colors ${
                              choice === 'combo'
                                ? 'border-zinc-900 bg-zinc-900 dark:border-zinc-100 dark:bg-zinc-100'
                                : 'border-zinc-300 dark:border-zinc-600'
                            }`}
                          >
                            {choice === 'combo' && <div className="w-1.5 h-1.5 rounded-full bg-white dark:bg-zinc-900" />}
                          </div>
                          <div className="w-8 h-8 rounded-md bg-zinc-100 dark:bg-zinc-800 border border-zinc-200/70 dark:border-zinc-700/70 text-zinc-800 dark:text-zinc-200 flex items-center justify-center shrink-0 text-sm">
                            <SwapOutlined />
                          </div>
                          <div className="min-w-0">
                            <div className="text-xs sm:text-sm font-medium text-zinc-950 dark:text-zinc-50 flex items-center gap-1.5 flex-wrap">
                              <span>{en ? 'Balance + Online Combo' : '余额抵扣 + 在线补差'}</span>
                              <span className="text-xs font-normal text-zinc-500 dark:text-zinc-400">
                                {en
                                  ? `(Balance: ${formatCreditMoney(symbol, order.balance)})`
                                  : `(可用余额: ${formatCreditMoney(symbol, order.balance)})`}
                              </span>
                            </div>
                            <div className="text-[11px] text-zinc-500 dark:text-zinc-400 truncate mt-0.5">
                              {en
                                ? `Balance ${formatCreditMoney(symbol, order.combo_balance)} + Online ${formatCreditMoney(symbol, order.combo_online)}`
                                : `余额扣 ${formatCreditMoney(symbol, order.combo_balance)} + 在线付 ${formatCreditMoney(symbol, order.combo_online)}`}
                            </div>
                          </div>
                        </div>
                        <div className="text-right shrink-0">
                          <div className="text-xs sm:text-sm font-bold text-zinc-950 dark:text-zinc-50">
                            {formatCreditMoney(symbol, order.price)}
                          </div>
                        </div>
                      </div>
                    )}
                  </div>
                </div>

                {order.benefit_locked ? (
                  <div className="mt-3 p-2.5 rounded-lg bg-zinc-50 dark:bg-[#1c1c1f] border border-zinc-200 dark:border-zinc-800 text-xs text-zinc-600 dark:text-zinc-400">
                    {en
                      ? 'This order already reserved a price. Payment grants the benefits confirmed at that time.'
                      : '这笔订单已经占用余额或拉起了支付，将按当时确认的价格和权益发放。'}
                  </div>
                ) : null}
                {order.sale_closed ? (
                  <div className="mt-3 p-2.5 rounded-lg bg-zinc-50 dark:bg-[#1c1c1f] border border-zinc-200 dark:border-zinc-800 text-xs text-zinc-600 dark:text-zinc-400">
                    {en ? 'This plan is off the shelf. This unpaid order can no longer be paid.' : '方案已下架，这笔未付款订单不能继续支付。'}
                  </div>
                ) : null}
                {order.cycle_closed ? (
                  <div className="mt-3 p-2.5 rounded-lg bg-zinc-50 dark:bg-[#1c1c1f] border border-zinc-200 dark:border-zinc-800 text-xs text-zinc-700 dark:text-zinc-300">
                    {en ? 'This cycle is no longer sold. Place a new order for a cycle that is still on sale.' : '这个周期已停售。请按当前仍在售的周期重新下单。'}
                  </div>
                ) : null}
                {order.content_changed && (order.changes || []).length > 0 ? (
                  <div className="mt-3 p-2.5 rounded-lg bg-zinc-50 dark:bg-[#1c1c1f] border border-zinc-200 dark:border-zinc-800 text-xs text-zinc-700 dark:text-zinc-300 space-y-0.5">
                    <div className="font-medium">{en ? 'The plan changed since you subscribed. This payment uses the new terms.' : '方案内容已更新。这一笔按现在的方案发放，已经买到的那一期不变。'}</div>
                    {(order.changes || []).slice(0, 6).map((row) => (
                      <div key={row.key} className="text-zinc-500 dark:text-zinc-400">{row.before} → {row.after}</div>
                    ))}
                  </div>
                ) : null}
                {preview && searchParams.get('cycle') && order.billing_cycle && searchParams.get('cycle') !== order.billing_cycle ? (
                  <div className="mt-3 p-2.5 rounded-lg bg-zinc-50 dark:bg-[#1c1c1f] border border-zinc-200 dark:border-zinc-800 text-xs text-zinc-700 dark:text-zinc-300">
                    {en ? 'The previous cycle is no longer sold. A cycle that is still on sale is selected.' : '原来的周期已停售，已改为当前仍在售的周期。'}
                  </div>
                ) : null}

                {/* Order Billing Breakdown */}
                <div className="mt-4 pt-3 border-t border-zinc-200/80 dark:border-zinc-800 space-y-1.5 text-xs sm:text-sm">
                  <div className="flex justify-between text-zinc-500 dark:text-zinc-400">
                    <span>{en ? 'Item Price' : '商品原价'}</span>
                    <span>{formatCreditMoney(symbol, order.list_price || order.price)}</span>
                  </div>
                  {discountAmount > 0 && (
                    <div className="flex justify-between text-zinc-900 dark:text-zinc-100 font-medium">
                      <span>{en ? 'Discount' : '优惠减免'}</span>
                      <span>-{formatCreditMoney(symbol, discountAmount)}</span>
                    </div>
                  )}
                  <div className="flex justify-between items-baseline pt-2 border-t border-dashed border-zinc-200 dark:border-zinc-800">
                    <span className="font-semibold text-zinc-950 dark:text-zinc-50 text-xs sm:text-sm">
                      {en ? 'Total Due' : '应付金额'}
                    </span>
                    <span className="text-xl sm:text-2xl font-bold text-zinc-950 dark:text-zinc-50 tracking-tight">
                      {formatCreditMoney(symbol, order.price)}
                    </span>
                  </div>
                </div>

                {/* Primary Action Button */}
                {(order.sale_closed || order.cycle_closed) ? null : (
                <Button
                  type="primary"
                  size="large"
                  block
                  loading={paying}
                  disabled={(choice === 'balance' && !order.can_balance) || switchingCycle}
                  onClick={pay}
                  className="order-checkout-btn-primary mt-4"
                >
                  {paying
                    ? (en ? 'Processing Order...' : '正在处理支付...')
                    : order.can_renew
                    ? (order.content_changed ? (en ? 'Confirm updated renewal' : '确认新方案续费') : (en ? 'Confirm Renewal' : '确认续费'))
                    : (en ? `Pay Now · ${formatCreditMoney(symbol, order.price)}` : `立即支付 · ${formatCreditMoney(symbol, order.price)}`)}
                </Button>
                )}

                {/* Cancel Button (if unpaid) */}
                {order.status === 'unpaid' && (
                  <Button
                    block
                    size="small"
                    disabled={paying}
                    onClick={cancel}
                    className="mt-2 !h-9 !rounded-lg text-xs !text-zinc-600 dark:!text-zinc-400 hover:!text-zinc-900 dark:hover:!text-zinc-100 !border-zinc-200 dark:!border-zinc-800 !bg-transparent hover:!bg-zinc-100 dark:hover:!bg-zinc-800/60"
                  >
                    {en ? 'Cancel Unpaid Order' : '取消未付款订单'}
                  </Button>
                )}
              </Spin>
            )}

            {order.status === 'paid' && (
              <div className="mt-3.5">
                <div className="p-3.5 rounded-lg bg-zinc-50 dark:bg-[#1c1c1f] border border-zinc-200 dark:border-zinc-800 flex items-start gap-2.5">
                  <CheckCircleFilled className="text-zinc-900 dark:text-zinc-100 text-base shrink-0 mt-0.5" />
                  <div className="min-w-0 flex-1">
                    <div className="text-xs sm:text-sm font-semibold text-zinc-950 dark:text-zinc-50">
                      {en ? 'Payment Received' : '已成功完成支付'}
                    </div>
                    <div className="text-xs text-zinc-500 dark:text-zinc-400 mt-0.5">
                      {order.tier_note || (en ? 'The coupon has been added to your account.' : '卡券已成功发放到您的账户中，相关特权已立即生效。')}
                    </div>
                  </div>
                </div>

                <div className="mt-4 pt-3 border-t border-zinc-200/80 dark:border-zinc-800 space-y-1.5 text-xs sm:text-sm">
                  {order.order_no && (
                    <div className="flex justify-between text-zinc-500 dark:text-zinc-400">
                      <span>{en ? 'Order No.' : '订单编号'}</span>
                      <span className="text-zinc-800 dark:text-zinc-200">{order.order_no}</span>
                    </div>
                  )}
                  {(order.paid_at || order.created_at) && (
                    <div className="flex justify-between text-zinc-500 dark:text-zinc-400">
                      <span>{en ? 'Paid Time' : '支付时间'}</span>
                      <span className="text-zinc-800 dark:text-zinc-200">
                        {formatApiDateTime(order.paid_at || order.created_at, 'YYYY-MM-DD HH:mm')}
                      </span>
                    </div>
                  )}
                  <div className="flex justify-between items-baseline pt-2 border-t border-dashed border-zinc-200 dark:border-zinc-800">
                    <span className="font-semibold text-zinc-950 dark:text-zinc-50 text-xs sm:text-sm">
                      {en ? 'Amount Paid' : '实付金额'}
                    </span>
                    <span className="text-xl sm:text-2xl font-bold text-zinc-950 dark:text-zinc-50 tracking-tight">
                      {formatCreditMoney(symbol, order.price)}
                    </span>
                  </div>
                </div>

                <Button
                  type="primary"
                  size="large"
                  block
                  onClick={() => navigate('/credits/records')}
                  className="order-checkout-btn-primary mt-4"
                >
                  {en ? 'View Coupon Records' : '查看卡券记录'}
                </Button>
              </div>
            )}

            {order.status === 'closed' && (
              <div className="mt-3.5">
                <div className="p-3.5 rounded-lg bg-zinc-50 dark:bg-[#1c1c1f] border border-zinc-200 dark:border-zinc-800 flex items-start gap-2.5 text-zinc-500 dark:text-zinc-400">
                  <CloseCircleFilled className="text-zinc-400 dark:text-zinc-500 text-base shrink-0 mt-0.5" />
                  <div className="min-w-0 flex-1">
                    <div className="text-xs sm:text-sm font-semibold text-zinc-800 dark:text-zinc-200">
                      {en ? 'Order Closed' : '订单已关闭'}
                    </div>
                    <div className="text-xs mt-0.5">
                      {en ? 'This order was cancelled or timed out. Any reserved balance has been returned.' : '该订单已取消或超时失效。如有预占用的余额已自动退回您的账户。'}
                    </div>
                  </div>
                </div>

                <div className="mt-4 pt-3 border-t border-zinc-200/80 dark:border-zinc-800 space-y-1.5 text-xs sm:text-sm">
                  {order.order_no && (
                    <div className="flex justify-between text-zinc-500 dark:text-zinc-400">
                      <span>{en ? 'Order No.' : '订单编号'}</span>
                      <span className="text-zinc-800 dark:text-zinc-200">{order.order_no}</span>
                    </div>
                  )}
                  {order.created_at && (
                    <div className="flex justify-between text-zinc-500 dark:text-zinc-400">
                      <span>{en ? 'Created Time' : '创建时间'}</span>
                      <span className="text-zinc-800 dark:text-zinc-200">
                        {formatApiDateTime(order.created_at, 'YYYY-MM-DD HH:mm')}
                      </span>
                    </div>
                  )}
                  <div className="flex justify-between items-baseline pt-2 border-t border-dashed border-zinc-200 dark:border-zinc-800">
                    <span className="font-semibold text-zinc-950 dark:text-zinc-50 text-xs sm:text-sm">
                      {en ? 'Order Amount' : '订单金额'}
                    </span>
                    <span className="text-xl sm:text-2xl font-bold text-zinc-500 dark:text-zinc-400 tracking-tight">
                      {formatCreditMoney(symbol, order.price)}
                    </span>
                  </div>
                </div>

                <Button
                  type="primary"
                  size="large"
                  block
                  onClick={() => navigate('/credits')}
                  className="order-checkout-btn-primary mt-4"
                >
                  {en ? 'Explore Other Plans' : '重新选购方案'}
                </Button>
              </div>
            )}

            {/* Trust Footer */}
            <div className="mt-4 pt-3 border-t border-zinc-200/60 dark:border-zinc-800/60 flex items-center justify-center gap-3 sm:gap-4 text-[10px] sm:text-[11px] text-zinc-400 dark:text-zinc-500">
              <span className="flex items-center gap-1">
                <SafetyCertificateOutlined className="text-zinc-400 dark:text-zinc-500" />
                {en ? 'Encrypted' : '安全加密'}
              </span>
              <span>·</span>
              <span className="flex items-center gap-1">
                <ThunderboltOutlined className="text-zinc-400 dark:text-zinc-500" />
                {en ? 'Instant' : '极速发货'}
              </span>
              <span>·</span>
              <span className="flex items-center gap-1">
                <ClockCircleOutlined className="text-zinc-400 dark:text-zinc-500" />
                {en ? '24/7 Auto' : '全天候响应'}
              </span>
            </div>
          </div>
        </div>
      </div>
      <RechargeModal
        visible={onlineOpen}
        couponOrderNo={order?.order_no || activeOrderNo}
        fixedAmount={onlineAmount}
        onCancel={() => setOnlineOpen(false)}
        onSuccess={() => {
          setOnlineOpen(false);
          void load(order?.order_no || activeOrderNo);
        }}
      />
    </div>
  );
};

export default OrderDetail;
