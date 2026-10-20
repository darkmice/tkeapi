/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useCallback, useEffect, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Card, Typography, Row, Col, Table, Button, Tag, Grid, Tabs, Input, InputNumber, Modal, message, Space, Tooltip } from 'antd';
import {
  SwapOutlined,
  HistoryOutlined,
  CopyOutlined,
  TeamOutlined,
  GiftOutlined,
  WalletOutlined,
  PlusOutlined,
  TagOutlined,
} from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import request from '../../utils/request';
import useSettingsStore from '../../store/settings';
import useAuthStore from '../../store/auth';
import { listPagination } from '../../components/ListPagination';
import { useThemeStore } from '../../store/theme';
import type { WalletStats, RechargeRecord } from '../../types';
import dayjs from 'dayjs';

import RechargeModal from './RechargeModal';
import { rechargeTypeFilters, rechargeTypeLabel, extractOrderNo, cleanRemark } from '../../utils/rechargeType';
import { copyWithFeedback } from '../../utils/clipboard';
import { formatApiDateTime } from '../../utils/timedisplay';
import PageHeader from '../../components/PageHeader';

const { Title, Text } = Typography;
const { useBreakpoint } = Grid;

interface AffiliateSummary {
  total_referred: number;
  active_referred: number;
  today_referred: number;
  month_referred: number;
  commission_earned: number;
  commission_clawback: number;
  commission_net: number;
  today_net: number;
  month_net: number;
  source_consumption: number;
  source_recharge: number;
  source_video: number;
  source_refund: number;
  signup_reward: number;
  signup_reward_count: number;
  today_signup_reward: number;
}

interface AffiliateReferral {
  uid: string;
  username: string;
  is_active: number;
  created_at: string;
  recharge_total: number;
  commission_net: number;
  commission_count: number;
}

interface AffiliateLedgerItem {
  id: number;
  created_at: string;
  from_username: string;
  from_uid: string;
  source: string;
  ratio: number;
  amount: number;
  order_no?: string;
}

interface AffiliatePage<T> {
  total: number;
  page: number;
  page_size: number;
  items: T[];
}

interface AffiliateOverview {
  enabled: boolean;
  marketing_enabled: boolean;
  commission_ratio: number;
  commission_basis: string;
  invite_reward_inviter: number;
  invite_reward_invitee: number;
  daily_invite_limit: number;
  link_clicks: number;
  commission_balance: number;
  summary: AffiliateSummary;
  referrals: AffiliatePage<AffiliateReferral>;
  ledger: AffiliatePage<AffiliateLedgerItem>;
}

const Wallet: React.FC = () => {
  const { t, i18n } = useTranslation();
  const isEn = i18n.language === 'en';
  const { settings } = useSettingsStore();
  const { user } = useAuthStore();
  const screens = useBreakpoint();

  const currencySymbol = settings?.currency?.currency_symbol || '$';
  const currencyUnit = settings?.currency?.currency_unit || '元';

  const { themeMode } = useThemeStore();
  const isLight = themeMode === 'light';

  const subText = isLight ? '#71717a' : '#a1a1aa';
  const mainText = isLight ? '#09090b' : '#fafafa';
  const cardBg = isLight ? '#ffffff' : '#09090b';
  const statCardBg = isLight ? '#ffffff' : '#141414';
  const cardBorder = `1px solid ${isLight ? '#e4e4e7' : '#27272a'}`;
  const neutralBg = isLight ? '#f4f4f5' : '#27272a';

  const [rechargeModalVisible, setRechargeModalVisible] = useState(false);
  const [stats, setStats] = useState<WalletStats | null>(null);
  const [records, setRecords] = useState<RechargeRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [availableModels, setAvailableModels] = useState<any[]>([]);
  const [transferring, setTransferring] = useState(false);
  const [transferOpen, setTransferOpen] = useState(false);
  const [transferAmount, setTransferAmount] = useState<number | null>(null);

  // 邀请返利与流水数据
  const [affiliateData, setAffiliateData] = useState<AffiliateOverview | null>(null);
  const [affiliateLoading, setAffiliateLoading] = useState(false);
  const [referralPage, setReferralPage] = useState(1);
  const [referralPageSize, setReferralPageSize] = useState(20);
  const [ledgerPage, setLedgerPage] = useState(1);
  const [ledgerPageSize, setLedgerPageSize] = useState(20);

  const [searchParams, setSearchParams] = useSearchParams();
  const currentTab = searchParams.get('tab') || 'records';

  const fetchAffiliate = useCallback(async (refPage = referralPage, refPageSize = referralPageSize, ledPage = ledgerPage, ledPageSize = ledgerPageSize) => {
    setAffiliateLoading(true);
    try {
      const resp = await (request.get('/user/affiliate/overview', {
        params: {
          referral_page: refPage,
          referral_page_size: refPageSize,
          ledger_page: ledPage,
          ledger_page_size: ledPageSize,
        },
      }) as unknown as Promise<AffiliateOverview>);
      setAffiliateData(resp);
    } catch (e) {
      console.error(e);
    } finally {
      setAffiliateLoading(false);
    }
  }, [referralPage, referralPageSize, ledgerPage, ledgerPageSize]);

  const fetchData = useCallback(async () => {
    setLoading(true);
    try {
      const [modelsResp, statsResp, recordsResp] = await Promise.all([
        request.get('/models') as unknown as Promise<any>,
        request.get('/user/wallet') as unknown as Promise<WalletStats>,
        request.get('/user/recharge_records') as unknown as Promise<RechargeRecord[]>,
      ]);
      if (modelsResp?.data) {
        setAvailableModels(modelsResp.data);
      }
      setStats(statsResp);
      setRecords(recordsResp || []);

      const affOn = Boolean(
        statsResp?.marketing_enabled ||
        (statsResp?.commission_ratio ?? 0) > 0 ||
        (statsResp?.commission_balance ?? 0) > 0
      );
      if (affOn) {
        fetchAffiliate(referralPage, referralPageSize, ledgerPage, ledgerPageSize);
      }
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  }, [fetchAffiliate, referralPage, referralPageSize, ledgerPage, ledgerPageSize]);

  const handleTransferCommission = async () => {
    const balance = stats?.commission_balance || 0;
    const minAmt = stats?.commission_transfer_min || 0;
    const maxAmt = stats?.commission_transfer_max || 0;
    const amount = Number(transferAmount);
    if (transferring || !(amount > 0)) {
      message.warning(isEn ? 'Enter an amount to transfer' : '请输入划转金额');
      return Promise.reject(new Error('invalid'));
    }
    if (minAmt > 0 && (balance < minAmt || amount < minAmt)) {
      message.warning(isEn ? `Each transfer must be at least ${currencySymbol}${minAmt}` : `单次最少划转 ${currencySymbol}${minAmt}，且余额需达到该金额`);
      return Promise.reject(new Error('invalid'));
    }
    if (maxAmt > 0 && amount > maxAmt) {
      message.warning(isEn ? `Each transfer can be at most ${currencySymbol}${maxAmt}` : `单次最多划转 ${currencySymbol}${maxAmt}`);
      return Promise.reject(new Error('invalid'));
    }
    if (amount > balance) {
      message.warning(isEn ? 'Amount exceeds the commission wallet' : '划转金额不能超过佣金钱包余额');
      return Promise.reject(new Error('invalid'));
    }
    setTransferring(true);
    try {
      const resp = await (request.post('/user/affiliate/transfer', { amount }) as any);
      const transferred = Number(resp?.amount ?? amount);
      message.success(isEn
        ? `Transferred ${currencySymbol}${transferred.toFixed(6)} to the system wallet`
        : `已划转 ${currencySymbol}${transferred.toFixed(6)} 到系统钱包`);
      setTransferAmount(null);
      setTransferOpen(false);
      await Promise.all([
        fetchData(),
        fetchAffiliate(referralPage, referralPageSize, ledgerPage, ledgerPageSize),
      ]);
    } catch (e) {
      console.error(e);
      throw e;
    } finally {
      setTransferring(false);
    }
  };

  const inviteLink = `${window.location.origin}/register?aff=${user?.uid || ''}`;

  const copyInviteLink = async () => {
    await copyWithFeedback(inviteLink, t('wallet.copy_invite_link_success', '邀请链接已复制到剪贴板'), t('wallet.copy_failed', '复制失败，请手动选择复制'));
  };

  useEffect(() => {
    fetchData();
  }, []);

  useEffect(() => {
    const payment = searchParams.get('payment');
    if (payment === 'success') {
      message.success(t('wallet.payment_success'));
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev);
        next.delete('payment');
        return next;
      }, { replace: true });
      const timer = setTimeout(() => fetchData(), 3000);
      return () => clearTimeout(timer);
    } else if (payment === 'cancelled') {
      message.info(t('wallet.payment_cancelled'));
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev);
        next.delete('payment');
        return next;
      }, { replace: true });
    }
  }, [searchParams]);

  const handleTabChange = (key: string) => {
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      if (key === 'records') {
        next.delete('tab');
      } else {
        next.set('tab', key);
      }
      return next;
    }, { replace: true });
  };

  const columns: any[] = [
    {
      title: t('wallet.order_no', { defaultValue: '订单号' }),
      key: 'order_no',
      align: 'left' as const,
      onHeaderCell: () => ({ style: { paddingLeft: 15 } }),
      onCell: () => ({ style: { paddingLeft: 15 } }),
      render: (_: unknown, record: RechargeRecord) => {
        const orderNo = extractOrderNo(record);
        if (orderNo) {
          return (
            <Text style={{ fontFamily: 'monospace', fontSize: 12, color: mainText }}>
              {orderNo}
            </Text>
          );
        }
        return <Text style={{ color: subText }}>-</Text>;
      },
    },
    {
      title: isEn ? 'Type' : '资金类型',
      dataIndex: 'recharge_type',
      key: 'recharge_type',
      align: 'center' as const,
      filters: rechargeTypeFilters(),
      onFilter: (value: string | number | boolean, record: RechargeRecord) =>
        record.recharge_type === value,
      render: (type: string, record: RechargeRecord) => (
        <Tag
          bordered={false}
          style={{
            margin: 0,
            fontSize: 12,
            fontFamily: 'monospace',
            background: isLight ? '#f4f4f5' : '#27272a',
            color: mainText,
            border: `1px solid ${isLight ? '#e4e4e7' : '#3f3f46'}`,
          }}
        >
          {rechargeTypeLabel(type, record.wallet_type)}
        </Tag>
      ),
    },
    {
      title: t('wallet.amount', { defaultValue: `金额 (${currencyUnit})`, unit: currencyUnit }),
      dataIndex: 'amount',
      key: 'amount',
      align: 'right' as const,
      filters: [
        { text: isEn ? 'Increase' : '增加', value: 'positive' },
        { text: isEn ? 'Decrease' : '扣除', value: 'negative' },
      ],
      onFilter: (value: string | number | boolean, record: RechargeRecord) => {
        if (value === 'positive') return record.amount > 0;
        if (value === 'negative') return record.amount < 0;
        return true;
      },
      render: (amount: number) => (
        <Text style={{ color: mainText, fontWeight: 500 }}>
          {amount > 0 ? '+' : '-'}{currencySymbol}{Math.abs(amount).toFixed(6)}
        </Text>
      ),
    },
    {
      title: t('wallet.time'),
      dataIndex: 'created_at',
      key: 'created_at',
      align: 'left' as const,
      render: (text: string) => <Text style={{ color: subText }}>{dayjs(text).format('YYYY/MM/DD HH:mm:ss')}</Text>,
    },
    {
      title: t('wallet.remark', { defaultValue: '备注' }),
      dataIndex: 'remark',
      key: 'remark',
      align: 'left' as const,
      render: (_: unknown, record: RechargeRecord) => {
        const cleaned = cleanRemark(record.remark, record.order_no);
        return <Text style={{ color: cleaned ? mainText : subText }}>{cleaned || '-'}</Text>;
      },
    },
  ];

  const commissionSourceLabel = (source: string) => {
    if (source === 'consumption') return t('affiliate_page.source_consumption', '消费返佣');
    if (source === 'recharge') return t('affiliate_page.source_recharge', '充值返佣');
    if (source === 'video') return t('affiliate_page.source_video', '视频消费返佣');
    if (source === 'refund') return t('affiliate_page.source_refund', '退款回冲');
    return t('affiliate_page.source_other', '其他');
  };

  const commissionSourceFilters = () => [
    { text: isEn ? 'Spending' : '消费返佣', value: 'consumption' },
    { text: isEn ? 'Top-up' : '充值返佣', value: 'recharge' },
    { text: isEn ? 'Video' : '视频消费返佣', value: 'video' },
    { text: isEn ? 'Refund' : '退款回冲', value: 'refund' },
  ];

  const ledgerColumns: any[] = [
    {
      title: t('wallet.order_no', { defaultValue: '订单号' }),
      key: 'order_no',
      align: 'left' as const,
      onHeaderCell: () => ({ style: { paddingLeft: 15 } }),
      onCell: () => ({ style: { paddingLeft: 15 } }),
      render: (_: unknown, record: AffiliateLedgerItem) => {
        const orderNo = extractOrderNo(record);
        if (orderNo) {
          return (
            <Text style={{ fontFamily: 'monospace', fontSize: 12, color: mainText }}>
              {orderNo}
            </Text>
          );
        }
        return <Text style={{ color: subText }}>-</Text>;
      },
    },
    {
      title: isEn ? 'Type' : '资金类型',
      dataIndex: 'source',
      key: 'source',
      align: 'center' as const,
      filters: commissionSourceFilters(),
      onFilter: (value: string | number | boolean, record: AffiliateLedgerItem) =>
        record.source === value,
      render: (source: string) => (
        <Tag
          bordered={false}
          style={{
            margin: 0,
            fontSize: 12,
            background: isLight ? '#f4f4f5' : '#27272a',
            color: mainText,
            border: `1px solid ${isLight ? '#e4e4e7' : '#3f3f46'}`,
          }}
        >
          {commissionSourceLabel(source)}
        </Tag>
      ),
    },
    {
      title: t('wallet.amount', { defaultValue: `金额 (${currencyUnit})`, unit: currencyUnit }),
      dataIndex: 'amount',
      key: 'amount',
      align: 'right' as const,
      filters: [
        { text: isEn ? 'Increase' : '增加', value: 'positive' },
        { text: isEn ? 'Decrease' : '扣除', value: 'negative' },
      ],
      onFilter: (value: string | number | boolean, record: AffiliateLedgerItem) => {
        if (value === 'positive') return record.amount > 0;
        if (value === 'negative') return record.amount < 0;
        return true;
      },
      render: (amount: number) => (
        <Text style={{ color: mainText, fontWeight: 500 }}>
          {amount > 0 ? '+' : '-'}{currencySymbol}{Math.abs(amount).toFixed(6)}
        </Text>
      ),
    },
    {
      title: t('wallet.time', { defaultValue: '时间' }),
      dataIndex: 'created_at',
      key: 'created_at',
      align: 'left' as const,
      render: (text: string) => <Text style={{ color: subText }}>{dayjs(text).format('YYYY/MM/DD HH:mm:ss')}</Text>,
    },
    {
      title: t('wallet.remark', { defaultValue: '备注' }),
      key: 'remark',
      align: 'left' as const,
      render: (_: unknown, record: AffiliateLedgerItem) => {
        const userLabel = record.from_username || record.from_uid || '-';
        const ratioStr = record.ratio > 0 ? `${Math.round(record.ratio * 100)}%` : '';
        return (
          <Text style={{ color: mainText }}>
            {isEn ? 'From: ' : '来源用户: '}
            <span style={{ fontWeight: 500 }}>{userLabel}</span>
            {record.from_uid && record.from_username ? (
              <Text style={{ color: subText, fontSize: 12, marginLeft: 4 }}>({record.from_uid})</Text>
            ) : null}
            {ratioStr ? (
              <Tag bordered={false} style={{ marginLeft: 6, fontSize: 11, padding: '0 4px' }}>
                {isEn ? `Rate: ${ratioStr}` : `比例: ${ratioStr}`}
              </Tag>
            ) : null}
          </Text>
        );
      },
    },
  ];

  const referralColumns: any[] = [
    {
      title: t('affiliate_page.col_user', '用户'),
      dataIndex: 'username',
      key: 'username',
      onHeaderCell: () => ({ style: { paddingLeft: 15 } }),
      onCell: () => ({ style: { paddingLeft: 15 } }),
      render: (_: string, row: AffiliateReferral) => (
        <div>
          <div style={{ color: mainText, fontWeight: 500 }}>{row.username || row.uid}</div>
          <Text style={{ color: subText, fontSize: 12 }}>{row.uid}</Text>
        </div>
      ),
    },
    {
      title: t('affiliate_page.col_status', '状态'),
      dataIndex: 'is_active',
      key: 'is_active',
      width: 100,
      render: (active: number) => (
        <Tag
          bordered={false}
          style={{
            margin: 0,
            fontSize: 12,
            background: active === 1 ? (isLight ? '#18181b' : '#fafafa') : (isLight ? '#f4f4f5' : '#27272a'),
            color: active === 1 ? (isLight ? '#fafafa' : '#18181b') : subText,
            border: `1px solid ${isLight ? '#e4e4e7' : '#3f3f46'}`,
          }}
        >
          {active === 1 ? t('affiliate_page.active', '有效') : t('affiliate_page.inactive', '普通')}
        </Tag>
      ),
    },
    {
      title: t('affiliate_page.col_joined', '注册时间'),
      dataIndex: 'created_at',
      key: 'created_at',
      width: 180,
      render: (value: string) => <span style={{ whiteSpace: 'nowrap' }}>{formatApiDateTime(value)}</span>,
    },
    {
      title: t('affiliate_page.col_recharge', '累计充值'),
      dataIndex: 'recharge_total',
      key: 'recharge_total',
      align: 'right' as const,
      render: (value: number) => `${currencySymbol}${(value || 0).toFixed(6)}`,
    },
    {
      title: t('affiliate_page.col_commission', '累计返佣'),
      dataIndex: 'commission_net',
      key: 'commission_net',
      align: 'right' as const,
      render: (value: number) => (
        <span style={{ color: value < 0 ? (isLight ? '#dc2626' : '#f87171') : mainText, fontWeight: 500 }}>
          {currencySymbol}${(value || 0).toFixed(6)}
        </span>
      ),
    },
  ];

  const modelDiscounts: Record<string, number> = user?.model_discounts ? (() => { try { return JSON.parse(user.model_discounts); } catch { return {}; } })() : {};
  const discountEntries = Object.entries(modelDiscounts);

  const modelDiscountData = discountEntries.map(([mid, discount]) => {
    const model = availableModels.find((m: any) => m.mid === mid);
    const hasLimit = model?.site_discount_enabled === 1;
    const limit = hasLimit ? Number(model.site_discount || 1) : null;
    const isLimited = hasLimit && discount < limit!;
    const actualDiscount = isLimited ? limit : discount;
    return {
      mid,
      model_id: model ? model.model_id : '-',
      modelName: model ? model.name : mid,
      discount,
      limit,
      hasLimit,
      isLimited,
      actualDiscount,
    };
  });

  const modelColumns = [
    {
      title: isEn ? 'Model(MID)' : '模型(MID)',
      dataIndex: 'mid',
      key: 'mid',
      onHeaderCell: () => ({ style: { paddingLeft: 15 } }),
      onCell: () => ({ style: { paddingLeft: 15 } }),
      render: (text: string) => <Text style={{ color: subText, fontSize: 13, fontFamily: 'monospace' }}>{text}</Text>,
    },
    {
      title: isEn ? 'Model Name' : '模型名称',
      dataIndex: 'modelName',
      key: 'modelName',
      render: (text: string) => <Text style={{ color: mainText, fontWeight: 500 }}>{text}</Text>,
    },
    {
      title: isEn ? 'Request ID' : '请求 ID',
      dataIndex: 'model_id',
      key: 'model_id',
      render: (text: string) => (
        <Text 
          copyable={!!text && text !== '-' ? { text: String(text) } : false} 
          style={{ color: subText, fontSize: 13, fontFamily: 'monospace' }}
        >
          {text || '-'}
        </Text>
      ),
    },
    {
      title: isEn ? 'Exclusive Discount' : '专享折扣',
      dataIndex: 'actualDiscount',
      key: 'discount',
      align: 'right' as const,
      render: (_: number, record: (typeof modelDiscountData)[number]) => (
        <Space size={6}>
          <Text style={{ color: mainText }}>
            {record.actualDiscount}x
          </Text>
          {record.isLimited && (
            <Tooltip title={isEn ? `Configured ${record.discount}x, clamped by model floor price ${record.limit}x` : `原设 ${record.discount}x，受模型保底限价 ${record.limit}x 约束`}>
              <Tag color="orange" style={{ margin: 0, fontSize: 11, lineHeight: '18px', padding: '0 4px' }}>
                {isEn ? `Floor ${record.limit}x` : `保底 ${record.limit}x`}
              </Tag>
            </Tooltip>
          )}
        </Space>
      ),
    },
  ];

  const hasAffiliate = Boolean(
    stats?.marketing_enabled ||
    (stats?.commission_ratio ?? 0) > 0 ||
    (stats?.commission_balance ?? 0) > 0
  );

  const totalAvailable = ((stats?.balance || 0) + (stats?.credit_limit || 0) + (stats?.gift_balance || 0)).toFixed(6);

  const tabItems = [
    {
      key: 'records',
      label: (
        <span style={{ fontSize: 14, fontWeight: 500 }}>
          <HistoryOutlined style={{ marginRight: 6 }} />
          {t('wallet.recharge_records', '充值记录')}
        </span>
      ),
      children: (
        <div style={{ borderRadius: 8, border: cardBorder, overflow: 'hidden', background: cardBg }}>
          <Table
            dataSource={records}
            columns={columns}
            rowKey="id"
            loading={loading}
            pagination={listPagination()}
            size="middle"
            scroll={{ x: 'max-content' }}
            style={{ margin: 0 }}
          />
        </div>
      ),
    },
    ...(hasAffiliate ? [
      {
        key: 'referrals',
        label: (
          <span style={{ fontSize: 14, fontWeight: 500 }}>
            <TeamOutlined style={{ marginRight: 6 }} />
            {t('affiliate_page.tab_referrals', '邀请用户')}
            {(affiliateData?.referrals?.total || 0) > 0 && (
              <Tag bordered={false} style={{ marginLeft: 6, borderRadius: 10, fontSize: 11, padding: '0 6px' }}>
                {affiliateData?.referrals?.total}
              </Tag>
            )}
          </span>
        ),
        children: (
          <div style={{ borderRadius: 8, border: cardBorder, overflow: 'hidden', background: cardBg }}>
            <Table<AffiliateReferral>
              rowKey="uid"
              loading={affiliateLoading}
              dataSource={affiliateData?.referrals?.items || []}
              pagination={listPagination({
                current: referralPage,
                pageSize: referralPageSize,
                total: affiliateData?.referrals?.total || 0,
                onChange: (page, pageSize) => {
                  const newPage = pageSize !== referralPageSize ? 1 : page;
                  setReferralPage(newPage);
                  setReferralPageSize(pageSize);
                  fetchAffiliate(newPage, pageSize, ledgerPage, ledgerPageSize);
                },
              })}
              size="middle"
              scroll={{ x: 'max-content' }}
              style={{ margin: 0 }}
              locale={{ emptyText: t('affiliate_page.empty_referrals', '暂无邀请用户') }}
              columns={referralColumns}
            />
          </div>
        ),
      },
      {
        key: 'ledger',
        label: (
          <span style={{ fontSize: 14, fontWeight: 500 }}>
            <WalletOutlined style={{ marginRight: 6 }} />
            {t('affiliate_page.tab_ledger', '返佣流水')}
            {(affiliateData?.ledger?.total || 0) > 0 && (
              <Tag bordered={false} style={{ marginLeft: 6, borderRadius: 10, fontSize: 11, padding: '0 6px' }}>
                {affiliateData?.ledger?.total}
              </Tag>
            )}
          </span>
        ),
        children: (
          <div style={{ borderRadius: 8, border: cardBorder, overflow: 'hidden', background: cardBg }}>
            <Table<AffiliateLedgerItem>
              rowKey="id"
              loading={affiliateLoading}
              dataSource={affiliateData?.ledger?.items || []}
              pagination={listPagination({
                current: ledgerPage,
                pageSize: ledgerPageSize,
                total: affiliateData?.ledger?.total || 0,
                onChange: (page, pageSize) => {
                  const newPage = pageSize !== ledgerPageSize ? 1 : page;
                  setLedgerPage(newPage);
                  setLedgerPageSize(pageSize);
                  fetchAffiliate(referralPage, referralPageSize, newPage, pageSize);
                },
              })}
              size="middle"
              scroll={{ x: 'max-content' }}
              style={{ margin: 0 }}
              locale={{ emptyText: t('affiliate_page.empty_ledger', '暂无返佣流水') }}
              columns={ledgerColumns}
            />
          </div>
        ),
      }
    ] : []),
    ...(discountEntries.length > 0 ? [
      {
        key: 'models',
        label: (
          <span style={{ fontSize: 14, fontWeight: 500 }}>
            <TagOutlined style={{ marginRight: 6 }} />
            {isEn ? 'Discounted Models' : '优惠模型'}
          </span>
        ),
        children: (
          <div style={{ borderRadius: 8, border: cardBorder, overflow: 'hidden', background: cardBg }}>
            <Table
              dataSource={modelDiscountData}
              columns={modelColumns}
              rowKey="mid"
              pagination={listPagination()}
              size="middle"
              scroll={{ x: 'max-content' }}
              style={{ margin: 0 }}
            />
          </div>
        ),
      }
    ] : [])
  ];

  const validTabKeys = tabItems.map((item) => item.key);
  const activeTabKey = validTabKeys.includes(currentTab) ? currentTab : 'records';

  return (
    <div style={{ maxWidth: 1200, margin: '0 auto', padding: screens.md ? '16px 0 40px 0' : '12px 0 24px 0' }}>
      {/* 统一标准页面头部 */}
      <PageHeader
        title={t('wallet.wallet_and_account', '钱包与账户')}
        description={isEn ? 'Manage your balance, top-ups, referral rewards, and commission ledger.' : '管理您的账户余额、充值记录、邀请返利及佣金明细。'}
        marginBottom={24}
      />

      {/* Main Balances Grid */}
      <Row gutter={[16, 16]} style={{ marginBottom: 24 }}>
        <Col xs={24} sm={12} lg={8}>
          <Card 
            style={{ borderRadius: 8, height: '100%', border: cardBorder, background: statCardBg, boxShadow: '0 1px 2px 0 rgba(0, 0, 0, 0.05)' }}
            styles={{ body: { padding: '20px' } }}
            bordered={false}
          >
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
              <Text style={{ color: subText, fontSize: 13, fontWeight: 500 }}>{isEn ? 'Total Available' : '可用总额'}</Text>
              <SwapOutlined style={{ color: subText }} />
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap' }}>
              <div style={{ fontSize: 26, fontWeight: 700, color: mainText, letterSpacing: '-0.025em', lineHeight: 1.1, wordBreak: 'break-word' }}>
                {currencySymbol}{totalAvailable}
              </div>
              {(!loading && stats?.pay_enabled !== false) && (
                <Button 
                  onClick={() => setRechargeModalVisible(true)}
                  style={{ 
                    borderRadius: 6, 
                    padding: '0 12px', 
                    height: 28, 
                    fontSize: 12,
                    fontWeight: 500, 
                    background: isLight ? '#09090b' : '#fafafa',
                    color: isLight ? '#fafafa' : '#09090b',
                    borderColor: isLight ? '#09090b' : '#fafafa',
                    borderWidth: 0,
                    display: 'flex',
                    alignItems: 'center',
                    gap: 4,
                    flexShrink: 0
                  }}
                >
                  <PlusOutlined /> {t('wallet.online_recharge', '在线充值')}
                </Button>
              )}
            </div>
            <div style={{ marginTop: 8 }}>
              <Text style={{ color: subText, fontSize: 12 }}>
                {isEn ? 'Spent: ' : '已消费: '}{currencySymbol}{(stats?.total_consumption || 0).toFixed(6)}
              </Text>
            </div>
          </Card>
        </Col>

        <Col xs={24} sm={12} lg={8}>
          <Card 
            style={{ borderRadius: 8, height: '100%', border: cardBorder, background: statCardBg, boxShadow: '0 1px 2px 0 rgba(0, 0, 0, 0.05)' }}
            styles={{ body: { padding: '20px' } }}
            bordered={false}
          >
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
              <Text style={{ color: subText, fontSize: 13, fontWeight: 500 }}>{t('wallet.system_balance', '系统总余额')}</Text>
              <WalletOutlined style={{ color: subText }} />
            </div>
            <div style={{ fontSize: 26, fontWeight: 700, color: mainText, letterSpacing: '-0.025em', lineHeight: 1.1 }}>
              {currencySymbol}{(stats?.balance || 0).toFixed(6)}
            </div>
            <div style={{ marginTop: 8 }}>
              {(stats?.credit_limit || 0) > 0 ? (
                <Text style={{ color: subText, fontSize: 12 }}>
                  {isEn ? 'Credit limit: ' : '信控金额: '}{currencySymbol}{(stats?.credit_limit || 0).toFixed(6)}
                </Text>
              ) : (
                <Text style={{ color: subText, fontSize: 12 }}>
                  {isEn ? 'System wallet' : '基础调用扣费钱包'}
                </Text>
              )}
            </div>
          </Card>
        </Col>

        <Col xs={24} sm={12} lg={8}>
          <Card 
            style={{ borderRadius: 8, height: '100%', border: cardBorder, background: statCardBg, boxShadow: '0 1px 2px 0 rgba(0, 0, 0, 0.05)' }}
            styles={{ body: { padding: '20px' } }}
            bordered={false}
          >
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
              <Text style={{ color: subText, fontSize: 13, fontWeight: 500 }}>{t('wallet.gift_balance', '赠送金余额')}</Text>
              <GiftOutlined style={{ color: subText }} />
            </div>
            <div style={{ fontSize: 26, fontWeight: 700, color: mainText, letterSpacing: '-0.025em', lineHeight: 1.1 }}>
              {currencySymbol}{(stats?.gift_balance || 0).toFixed(6)}
            </div>
            <div style={{ marginTop: 8 }}>
              <Text style={{ color: subText, fontSize: 12 }}>
                {t('wallet.priority_deduction', '优先扣除')}
              </Text>
            </div>
          </Card>
        </Col>
      </Row>

      {/* 邀请好友获返利：简洁整合卡片（含佣金钱包数据与划转） */}
      {hasAffiliate && (
        <Card
          style={{
            marginBottom: 20,
            borderRadius: 8,
            border: cardBorder,
            background: cardBg,
            boxShadow: '0 1px 2px 0 rgba(0, 0, 0, 0.05)',
          }}
          styles={{ body: { padding: '16px 20px' } }}
          bordered={false}
        >
          <div style={{
            display: 'flex',
            flexDirection: screens.lg ? 'row' : 'column',
            alignItems: screens.lg ? 'center' : 'stretch',
            justifyContent: 'space-between',
            gap: 16,
            marginBottom: 12,
          }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap' }}>
              <div style={{
                display: 'inline-flex',
                alignItems: 'center',
                justifyContent: 'center',
                width: 32,
                height: 32,
                borderRadius: 6,
                background: neutralBg,
                color: mainText,
              }}>
                <TeamOutlined style={{ fontSize: 16 }} />
              </div>
              <div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
                  <Text style={{ fontSize: 15, fontWeight: 600, color: mainText }}>
                    {t('wallet.invite_friends', '邀请好友获返利')}
                  </Text>
                  {stats?.marketing_enabled ? (
                    <Tag
                      bordered={false}
                      style={{
                        margin: 0,
                        fontSize: 12,
                        background: isLight ? '#18181b' : '#fafafa',
                        color: isLight ? '#fafafa' : '#18181b',
                      }}
                    >
                      {stats?.commission_basis === 'recharge' ? t('affiliate_page.basis_recharge', '按充值') : t('affiliate_page.basis_consumption', '按实际消费')}
                      {' · '}
                      {Math.round((stats?.commission_ratio || 0) * 100)}%
                    </Tag>
                  ) : (
                    <Tag bordered={false} style={{ margin: 0, fontSize: 12, background: neutralBg, color: subText }}>
                      {t('affiliate_page.awards_paused', '已停止发放')}
                    </Tag>
                  )}
                  {(affiliateData?.daily_invite_limit || 0) > 0 && (
                    <Tag bordered={false} style={{ margin: 0, fontSize: 12 }}>
                      {t('affiliate_page.daily_limit', '每日限额')}: {affiliateData?.summary?.today_referred || 0}/{affiliateData?.daily_invite_limit}
                    </Tag>
                  )}
                </div>
              </div>
            </div>

            <div style={{
              display: 'flex',
              gap: 8,
              alignItems: 'center',
              maxWidth: screens.lg ? 560 : '100%',
              width: screens.lg ? 'auto' : '100%',
              flex: screens.lg ? '1 1 auto' : undefined,
              justifyContent: screens.lg ? 'flex-end' : 'stretch',
            }}>
              <div style={{
                background: neutralBg,
                padding: '6px 12px',
                borderRadius: 6,
                border: cardBorder,
                flex: 1,
                minWidth: 0,
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
              }}>
                <Text style={{
                  color: mainText,
                  fontSize: 12,
                  fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
                }}>
                  {inviteLink}
                </Text>
              </div>
              <Button
                icon={<CopyOutlined />}
                onClick={copyInviteLink}
                style={{
                  height: 32,
                  borderRadius: 6,
                  fontSize: 13,
                  flexShrink: 0,
                }}
              >
                {t('affiliate_page.copy', '复制链接')}
              </Button>
            </div>
          </div>

          <div style={{
            display: 'flex',
            flexWrap: 'wrap',
            gap: screens.md ? '16px 28px' : '12px 16px',
            paddingTop: 12,
            borderTop: `1px dashed ${isLight ? '#f4f4f5' : '#27272a'}`,
            alignItems: 'center',
            fontSize: 13,
          }}>
            {/* 佣金钱包余额与划转 */}
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
              <div>
                <Text style={{ color: subText }}>{isEn ? 'Commission Wallet' : '佣金钱包'}: </Text>
                <Text style={{ color: mainText, fontWeight: 700, fontSize: 14 }}>
                  {currencySymbol}{(stats?.commission_balance || 0).toFixed(6)}
                </Text>
              </div>
              <Button
                size="small"
                onClick={() => {
                  setTransferAmount(null);
                  setTransferOpen(true);
                }}
                style={{
                  fontSize: 12,
                  height: 24,
                  padding: '0 8px',
                  borderRadius: 4,
                }}
              >
                {isEn ? 'Transfer' : '佣金划转'}
              </Button>
            </div>

            {/* 累计净返佣 */}
            <div>
              <Text style={{ color: subText }}>{t('affiliate_page.net', '累计净返佣')}: </Text>
              <Text style={{ color: mainText, fontWeight: 600 }}>
                {currencySymbol}{(affiliateData?.summary?.commission_net || 0).toFixed(6)}
              </Text>
              {(affiliateData?.summary?.commission_clawback || 0) > 0 && (
                <Text style={{ color: subText, fontSize: 12 }}>
                  {' '}({t('affiliate_page.clawback', '回冲')}: {currencySymbol}{affiliateData?.summary?.commission_clawback.toFixed(6)})
                </Text>
              )}
            </div>

            {/* 累计邀请 */}
            <div>
              <Text style={{ color: subText }}>{t('affiliate_page.referred', '累计邀请')}: </Text>
              <Text style={{ color: mainText, fontWeight: 600 }}>{affiliateData?.summary?.total_referred ?? (stats?.total_referred || 0)}</Text>
              <Text style={{ color: subText, fontSize: 12 }}>
                {' '}({t('affiliate_page.today_referred', '今日')}: {affiliateData?.summary?.today_referred || 0} · {t('affiliate_page.month_referred', '本月')}: {affiliateData?.summary?.month_referred || 0} · {t('affiliate_page.active_referred', '有效')}: {affiliateData?.summary?.active_referred || 0})
              </Text>
            </div>

            {/* 推广点击 */}
            {(affiliateData?.link_clicks || 0) > 0 && (
              <div>
                <Text style={{ color: subText }}>{t('affiliate_page.clicks', '推广点击')}: </Text>
                <Text style={{ color: mainText, fontWeight: 600 }}>{affiliateData?.link_clicks}</Text>
              </div>
            )}

            {/* 注册奖励 */}
            {(affiliateData?.summary?.signup_reward || 0) > 0 && (
              <div>
                <Text style={{ color: subText }}>{t('affiliate_page.signup_reward', '注册奖励')}: </Text>
                <Text style={{ color: mainText, fontWeight: 600 }}>{currencySymbol}{affiliateData?.summary?.signup_reward.toFixed(6)}</Text>
              </div>
            )}
          </div>
        </Card>
      )}



      {/* Tabs for Records, Referrals, Ledger & Models */}
      <div className="wallet-tab-table-wrapper">
        <style>{`
          .wallet-tab-table-wrapper .ant-table-thead > tr > th:first-child,
          .wallet-tab-table-wrapper .ant-table-tbody > tr:not(.ant-table-measure-row) > td:first-child {
            padding-left: 15px !important;
          }
        `}</style>
        <Tabs
          activeKey={activeTabKey}
          onChange={handleTabChange}
          tabBarStyle={{ borderBottom: cardBorder, marginBottom: 20 }}
          items={tabItems}
        />
      </div>

      <Modal
        title={isEn ? 'Transfer to system wallet' : '划转到系统钱包'}
        open={transferOpen}
        onCancel={() => {
          if (!transferring) setTransferOpen(false);
        }}
        onOk={handleTransferCommission}
        okText={isEn ? 'Confirm' : '确认划转'}
        confirmLoading={transferring}
        cancelText={isEn ? 'Cancel' : '取消'}
        destroyOnHidden
      >
        <div style={{ marginBottom: 12 }}>
          <Text style={{ color: subText }}>
            {isEn ? 'Commission wallet: ' : '佣金钱包余额：'}
          </Text>
          <Text strong>{currencySymbol}{(stats?.commission_balance || 0).toFixed(6)}</Text>
        </div>
        <InputNumber
          value={transferAmount}
          onChange={(value) => setTransferAmount(typeof value === 'number' ? value : null)}
          min={0}
          step={1}
          precision={6}
          placeholder={isEn ? 'Enter amount' : '请输入划转金额'}
          status={(stats?.commission_transfer_min || 0) > 0 && transferAmount != null && transferAmount > 0 && transferAmount < (stats?.commission_transfer_min || 0) ? 'error' : undefined}
          style={{ width: '100%' }}
          autoFocus
        />
        {(stats?.commission_transfer_min || 0) > 0 && transferAmount != null && transferAmount > 0 && transferAmount < (stats?.commission_transfer_min || 0) ? (
          <Text style={{ color: isLight ? '#dc2626' : '#f87171', fontSize: 12, display: 'block', marginTop: 8 }}>
            {isEn
              ? `Amount is not enough. Minimum is ${currencySymbol}${(stats?.commission_transfer_min || 0).toFixed(6)}.`
              : `金额不够，最少需要划转 ${currencySymbol}${(stats?.commission_transfer_min || 0).toFixed(6)}`}
          </Text>
        ) : (
          <Text style={{ color: subText, fontSize: 12, display: 'block', marginTop: 8 }}>
            {(stats?.commission_transfer_min || 0) > 0
              ? (isEn ? `Min ${currencySymbol}${stats?.commission_transfer_min}` : `最少 ${currencySymbol}${stats?.commission_transfer_min}`)
              : (isEn ? 'No minimum' : '最少不限')}
            {' · '}
            {(stats?.commission_transfer_max || 0) > 0
              ? (isEn ? `Max ${currencySymbol}${stats?.commission_transfer_max}` : `最多 ${currencySymbol}${stats?.commission_transfer_max}`)
              : (isEn ? 'No maximum' : '最多不限')}
          </Text>
        )}
      </Modal>

      <RechargeModal 
        visible={rechargeModalVisible}
        onCancel={() => setRechargeModalVisible(false)}
        onSuccess={() => {
          setRechargeModalVisible(false);
          fetchData();
        }}
      />
    </div>
  );
};

export default Wallet;
