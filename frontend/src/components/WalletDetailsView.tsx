/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState } from 'react';
import { Card, Table, Tag, Row, Col, Statistic, DatePicker, Tabs, Spin, Typography, Radio, Button } from 'antd';
import { useTranslation } from 'react-i18next';
import { useThemeStore } from '../store/theme';
import useSettingsStore from '../store/settings';
import request from '../utils/request';
import type { Dayjs } from 'dayjs';
import { formatApiDateTime, parseApiTimeAsUtc } from '../utils/timedisplay';
import { timedisplayNow, toDateRangeParams } from '../utils/dateRangeParams';
import { rechargeTypeLabel, extractOrderNo, cleanRemark } from '../utils/rechargeType';
import { listPagination } from './ListPagination';

const { Text } = Typography;

interface WalletDetailsViewProps {
  user: {
    id?: string | number;
    user_id?: string | number;
    username?: string;
    used_quota?: number;
    gift_used_quota?: number;
    credit_limit?: number;
    balance?: number;
    gift_balance?: number;
    commission_balance?: number;
  };
  recharges: any[];
  loading?: boolean;
  isLight?: boolean;
  currencySymbol?: string;
  /** 使用 team-marketing 专用 API 获取消费统计（用于推荐人查看推荐用户的消费数据） */
  useReferralApi?: boolean;
  /** 打开时落在哪个明细页。默认系统钱包。 */
  initialTab?: string;
}

const WalletDetailsView: React.FC<WalletDetailsViewProps> = ({
  user,
  recharges = [],
  loading = false,
  isLight: customIsLight,
  currencySymbol: customCurrencySymbol,
  useReferralApi = false,
  initialTab = 'system',
}) => {
  const { t } = useTranslation('team_marketing');
  
  // 动态读取主题
  const { themeMode } = useThemeStore();
  const isLight = customIsLight !== undefined ? customIsLight : themeMode === 'light';

  // 动态读取币种符号
  const { settings } = useSettingsStore();
  const currencySymbol = customCurrencySymbol !== undefined ? customCurrencySymbol : (settings?.currency?.currency_symbol || '$');

  // 组件局部状态控制日期过滤，使外部完全解耦
  const [activePreset, setActivePreset] = useState<string | null>('thisMonth');
  const [filterRange, setFilterRange] = useState<any>(() => [
    timedisplayNow().startOf('month'),
    timedisplayNow().endOf('month'),
  ]);

  const [consumedSystem, setConsumedSystem] = useState<number>(0);
  const [consumedGift, setConsumedGift] = useState<number>(0);
  const [consumedLoading, setConsumedLoading] = useState<boolean>(false);
  const [commissionLedger, setCommissionLedger] = useState<{
    commission_balance: number;
    period_earned: number;
    period_clawback: number;
    period_transfer: number;
    data: any[];
  } | null>(null);
  const [commissionLoading, setCommissionLoading] = useState<boolean>(false);
  const { start_date, end_date } = toDateRangeParams(filterRange);

  React.useEffect(() => {
    if (!user || !user.id) return;

    setConsumedLoading(true);

    // 推荐人查看推荐用户时，使用 team-marketing 专用 API
    if (useReferralApi) {
      const params: any = {};
      if (start_date) params.start_date = start_date;
      if (end_date) params.end_date = end_date;
      request.get(`/team-marketing/referral/${user.id}/consumption-stats`, { params }).then((res: any) => {
        setConsumedSystem(res?.total_system_cost || 0);
        setConsumedGift(res?.total_gift_cost || 0);
      }).catch(e => {
        console.error('Failed to fetch referral consumed amount', e);
        setConsumedSystem(0);
        setConsumedGift(0);
      }).finally(() => {
        setConsumedLoading(false);
      });
    } else {
      request.get('/logs', {
        params: {
          user_id: user.id,
          start_date,
          end_date,
          per_page: 1, // we just need the stats
        }
      }).then((res: any) => {
        setConsumedSystem(res?.total_system_cost || 0);
        setConsumedGift(res?.total_gift_cost || 0);
      }).catch(e => {
        console.error('Failed to fetch consumed amount', e);
        setConsumedSystem(0);
        setConsumedGift(0);
      }).finally(() => {
        setConsumedLoading(false);
      });
    }
  }, [start_date, end_date, user.id, useReferralApi]);

  const ledgerUserId = user?.id || user?.user_id;
  React.useEffect(() => {
    if (!ledgerUserId) return;
    let cancelled = false;
    setCommissionLoading(true);
    const params: Record<string, string> = {};
    if (start_date) params.start_date = start_date;
    if (end_date) params.end_date = end_date;
    request.get(`/users/${ledgerUserId}/commission-ledger`, { params }).then((res: any) => {
      if (cancelled) return;
      setCommissionLedger({
        commission_balance: Number(res?.commission_balance || 0),
        period_earned: Number(res?.period_earned || 0),
        period_clawback: Number(res?.period_clawback || 0),
        period_transfer: Number(res?.period_transfer || 0),
        data: Array.isArray(res?.data) ? res.data : [],
      });
    }).catch((e) => {
      console.error('Failed to fetch commission ledger', e);
      if (!cancelled) {
        setCommissionLedger({
          commission_balance: Number(user.commission_balance || 0),
          period_earned: 0,
          period_clawback: 0,
          period_transfer: 0,
          data: [],
        });
      }
    }).finally(() => {
      if (!cancelled) setCommissionLoading(false);
    });
    return () => { cancelled = true; };
  }, [ledgerUserId, start_date, end_date, user.commission_balance]);

  const handlePresetChange = (preset: string) => {
    setActivePreset(preset);
    const now = timedisplayNow();
    let range: [Dayjs, Dayjs] | null = null;
    switch (preset) {
      case 'today':
        range = [now.startOf('day'), now.clone().endOf('day')];
        break;
      case 'yesterday': {
        const day = now.subtract(1, 'day').startOf('day');
        range = [day, day.clone().endOf('day')];
        break;
      }
      case 'thisMonth':
        range = [now.startOf('month'), now.clone().endOf('month')];
        break;
      case 'lastMonth': {
        const month = now.subtract(1, 'month').startOf('month');
        range = [month, month.clone().endOf('month')];
        break;
      }
      case 'thisYear':
        range = [now.startOf('year'), now.clone().endOf('year')];
        break;
      case 'lastHalfYear':
        range = [now.clone().subtract(6, 'month').startOf('day'), now.clone().endOf('day')];
        break;
      case 'lastYear':
        range = [now.clone().subtract(1, 'year').startOf('day'), now.clone().endOf('day')];
        break;
      case 'all':
        range = null;
        break;
      default:
        range = null;
    }
    setFilterRange(range);
  };

  const handleRangeChange = (dates: any) => {
    setFilterRange(dates);
    setActivePreset(null); // 手动调节日期时清除快捷键高亮
  };

  const filteredRecharges = recharges.filter((r: any) => {
    if (!start_date || !end_date) return true;
    const rMs = parseApiTimeAsUtc(r.created_at)?.getTime();
    if (rMs == null) return false;
    return rMs >= Date.parse(start_date) && rMs <= Date.parse(end_date);
  });

  const isArkLedger = (type: string) => type === 'ark_video_consume' || type === 'ark_video_refund';
  const isPlatformConsume = (type: string) =>
    isArkLedger(type) ||
    type === 'coupon_purchase' ||
    type === 'coupon_pack' ||
    type === 'coupon_subscription' ||
    type === 'model_voucher';

  const systemRecharges = filteredRecharges.filter((r: any) => (r.wallet_type || 'system') === 'system');
  const giftRecharges = filteredRecharges.filter((r: any) => (r.wallet_type || 'system') === 'gift');
  const creditRecharges = filteredRecharges.filter((r: any) => (r.wallet_type || 'system') === 'credit');

  // 充值合计只统计充值/手动调账等入金，排除卡券加油包/订阅购买及方舟监控消费（但明细表格仍保留系统资金流水）
  const totalSystemRecharge = systemRecharges
    .filter((r: any) => !isPlatformConsume(r.recharge_type || ''))
    .reduce((sum, r) => sum + (Number(r.amount) || 0), 0);
  const totalGiftRecharge = giftRecharges
    .filter((r: any) => !isPlatformConsume(r.recharge_type || ''))
    .reduce((sum, r) => sum + (Number(r.amount) || 0), 0);
  const totalCreditRecharge = creditRecharges.reduce((sum, r) => sum + (Number(r.amount) || 0), 0);
  // 方舟流水：consume 为负、refund 为正 → 净消费 = -SUM(amount)；叠加到 /logs 消费合计
  const arkNetConsume = (rows: any[]) =>
    rows
      .filter((r: any) => isArkLedger(r.recharge_type || ''))
      .reduce((sum, r) => sum - (Number(r.amount) || 0), 0);
  const periodConsumedSystem = consumedSystem + arkNetConsume(systemRecharges);
  const periodConsumedGift = consumedGift + arkNetConsume(giftRecharges);
  
  const systemUsed = user.used_quota || 0;
  const giftUsed = user.gift_used_quota || 0;
  const creditLimit = user.credit_limit || 0;

  const getFilterTextPrefix = () => {
    if (activePreset === 'today') return '当日';
    if (activePreset === 'yesterday') return '前一日';
    if (activePreset === 'thisMonth') return '当月';
    if (activePreset === 'lastMonth') return '上月';
    if (activePreset === 'thisYear') return '当年';
    if (activePreset === 'lastHalfYear') return '近半年';
    if (activePreset === 'lastYear') return '近一年';
    if (activePreset === 'all') return '全部';
    if (filterRange && filterRange[0] && filterRange[1]) return '筛选期内';
    return '累计';
  };
  const timePrefix = getFilterTextPrefix();

  const statisticValueStyle = {
    color: isLight ? '#1f2937' : '#fff',
    fontSize: '15px',
    lineHeight: 1.2,
    whiteSpace: 'nowrap' as const,
    fontWeight: 600,
  };

  const getCreditValueStyle = () => ({
    color: isLight ? '#1890ff' : '#69b1ff',
    fontSize: '15px',
    lineHeight: 1.2,
    whiteSpace: 'nowrap' as const,
    fontWeight: 600,
  });

  const rechargeColumns = [
    {
      title: t('order_no', '订单号'),
      key: 'order_no',
      width: 200,
      render: (_: unknown, record: any) => {
        const orderNo = extractOrderNo(record);
        if (orderNo) {
          return (
            <Text
              copyable
              style={{
                display: 'inline-flex',
                alignItems: 'center',
                whiteSpace: 'nowrap',
                fontSize: 11,
                letterSpacing: '-0.2px',
              }}
            >
              {orderNo}
            </Text>
          );
        }
        return <Text type="secondary" style={{ fontSize: 11 }}>-</Text>;
      },
    },
    {
      title: t('recharge_type', '类型'),
      dataIndex: 'recharge_type',
      key: 'recharge_type',
      width: 120,
      render: (type: string, record: any) => (
        <Tag style={{ whiteSpace: 'nowrap' }}>
          {rechargeTypeLabel(type, record.wallet_type)}
        </Tag>
      ),
    },
    {
      title: t('amount', '金额'),
      dataIndex: 'amount',
      key: 'amount',
      width: 140,
      render: (amount: number) => (
        <Text style={{ color: amount > 0 ? '#52c41a' : amount < 0 ? '#ff4d4f' : undefined, fontWeight: 500, whiteSpace: 'nowrap' }}>
          {amount > 0 ? '+' : (amount < 0 ? '-' : '')}{Math.abs(amount).toFixed(6)}
        </Text>
      ),
    },
    {
      title: t('time', '时间'),
      dataIndex: 'created_at',
      key: 'created_at',
      width: 160,
      render: (tVal: string) => <Text style={{ fontSize: 12, whiteSpace: 'nowrap' }}>{formatApiDateTime(tVal, 'YYYY/MM/DD HH:mm:ss')}</Text>,
    },
    {
      title: t('remark', '备注'),
      dataIndex: 'remark',
      key: 'remark',
      width: 180,
      ellipsis: true,
      render: (_: unknown, record: any) => {
        const cleaned = cleanRemark(record.remark, record.order_no);
        return <span title={cleaned}>{cleaned || '-'}</span>;
      },
    },
    {
      title: t('operator', '操作人'),
      dataIndex: 'operator',
      key: 'operator',
      width: 100,
      render: (text: string) => <Text style={{ whiteSpace: 'nowrap' }}>{text || '-'}</Text>,
    },
  ];

  const commissionColumns = [
    {
      title: t('time', '时间'),
      dataIndex: 'created_at',
      key: 'created_at',
      width: 160,
      render: (tVal: string) => <Text style={{ fontSize: 12, whiteSpace: 'nowrap' }}>{formatApiDateTime(tVal, 'YYYY/MM/DD HH:mm:ss')}</Text>,
    },
    {
      title: t('recharge_type', '类型'),
      dataIndex: 'source_label',
      key: 'source_label',
      width: 120,
      render: (label: string, record: any) => {
        const color = record.source === 'transfer' ? 'gold'
          : record.source === 'refund' || record.source === 'adjust' ? 'volcano'
          : record.source === 'recharge' ? 'green'
          : record.source === 'video' ? 'purple'
          : 'blue';
        return <Tag color={color} style={{ whiteSpace: 'nowrap' }}>{label || '其他'}</Tag>;
      },
    },
    {
      title: t('amount', '金额'),
      dataIndex: 'amount',
      key: 'amount',
      width: 140,
      render: (amount: number) => (
        <Text style={{ color: amount > 0 ? '#52c41a' : amount < 0 ? '#ff4d4f' : undefined, fontWeight: 500, whiteSpace: 'nowrap' }}>
          {amount > 0 ? '+' : (amount < 0 ? '-' : '')}{currencySymbol}{Math.abs(Number(amount) || 0).toFixed(6)}
        </Text>
      ),
    },
    {
      title: '比例',
      dataIndex: 'ratio',
      key: 'ratio',
      width: 80,
      render: (ratio: number) => (Number(ratio) > 0 ? `${Math.round(Number(ratio) * 100)}%` : '-'),
    },
    {
      title: '来自用户',
      key: 'from_user',
      width: 180,
      render: (_: unknown, record: any) => {
        if (!record.from_username && !record.from_uid) return <Text type="secondary">-</Text>;
        return (
          <span>
            <Text style={{ whiteSpace: 'nowrap' }}>{record.from_username || '-'}</Text>
            {record.from_uid ? <Text type="secondary" style={{ marginLeft: 6, fontSize: 12 }}>{record.from_uid}</Text> : null}
          </span>
        );
      },
    },
  ];

  const commissionBalance = commissionLedger?.commission_balance ?? (user.commission_balance || 0);
  const commissionRows = commissionLedger?.data || [];

  if (loading) {
    return <div style={{ padding: 40, textAlign: 'center' }}><Spin /></div>;
  }

  // 构建 Tab 列表，信控额度 Tab 仅在有信控记录或额度 > 0 时显示
  const showCreditTab = creditRecharges.length > 0 || creditLimit > 0;

  return (
    <div className="compact-wallet-view">
      <style>{`
        .compact-wallet-view .ant-tabs-nav {
          margin-bottom: 8px !important;
        }
        .compact-wallet-view .ant-tabs-tab {
          padding: 5px 12px !important;
          font-size: 13px !important;
        }
        .compact-wallet-view .ant-statistic-title {
          margin-bottom: 2px !important;
          font-size: 11px !important;
        }
        .compact-wallet-view .ant-card-body {
          padding: 6px 10px !important;
        }
        .compact-wallet-view .ant-table-small .ant-table-thead > tr > th {
          padding: 5px 8px !important;
          font-size: 12px !important;
        }
        .compact-wallet-view .ant-table-small .ant-table-tbody > tr > td {
          padding: 4px 8px !important;
          font-size: 12px !important;
        }
        .compact-wallet-view .ant-table-small .ant-table-pagination.ant-pagination {
          margin: 6px 0 0 !important;
        }
      `}</style>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 6, marginBottom: 8 }}>
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, alignItems: 'center' }}>
          <Radio.Group 
            value={activePreset} 
            onChange={(e) => handlePresetChange(e.target.value)} 
            size="small"
            buttonStyle="outline"
          >
            <Radio.Button value="today">{t('today', '当日')}</Radio.Button>
            <Radio.Button value="yesterday">{t('yesterday', '前一日')}</Radio.Button>
            <Radio.Button value="thisMonth">{t('this_month', '当月')}</Radio.Button>
            <Radio.Button value="lastMonth">{t('last_month', '上月')}</Radio.Button>
            <Radio.Button value="thisYear">{t('this_year', '当年')}</Radio.Button>
            <Radio.Button value="lastHalfYear">{t('last_half_year', '最近半年')}</Radio.Button>
            <Radio.Button value="lastYear">{t('last_year', '最近一年')}</Radio.Button>
            <Radio.Button value="all">{t('all_time', '全部')}</Radio.Button>
          </Radio.Group>
          {filterRange && (
            <Button size="small" type="link" onClick={() => { setFilterRange(null); setActivePreset(null); }} style={{ padding: 0, fontSize: 12 }}>
              {t('reset', '重置')}
            </Button>
          )}
        </div>
        <DatePicker.RangePicker 
          value={filterRange} 
          onChange={handleRangeChange} 
          size="small"
          style={{ width: '100%' }} 
        />
      </div>

      <Tabs
        defaultActiveKey={initialTab}
        tabBarStyle={{ marginBottom: 8 }}
        items={[
        {
          key: 'system',
          label: t('system_wallet_details', '系统钱包明细'),
          children: (
            <div>
              <Row gutter={[8, 8]} style={{ marginBottom: 8 }}>
                <Col span={6}>
                  <Card size="small" style={{ background: isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', border: 'none' }}>
                    <Statistic title={<span style={{ color: isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>当前系统账户余额</span>} value={user.balance || 0} precision={6} prefix={currencySymbol} valueStyle={statisticValueStyle} />
                  </Card>
                </Col>
                <Col span={6}>
                  <Card size="small" style={{ background: isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', border: 'none' }}>
                    <Statistic title={<span style={{ color: isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>{`${timePrefix}充值记录`}</span>} value={totalSystemRecharge} precision={6} prefix={currencySymbol} valueStyle={statisticValueStyle} />
                  </Card>
                </Col>
                <Col span={6}>
                  <Card size="small" style={{ background: isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', border: 'none' }}>
                    <Statistic title={<span style={{ color: isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>{`${timePrefix}消费合计`}</span>} value={periodConsumedSystem} precision={6} prefix={currencySymbol} valueStyle={statisticValueStyle} loading={consumedLoading} />
                  </Card>
                </Col>
                <Col span={6}>
                  <Card size="small" style={{ background: isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', border: 'none' }}>
                    <Statistic title={<span style={{ color: isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>历史全部消费合计</span>} value={systemUsed} precision={6} prefix={currencySymbol} valueStyle={statisticValueStyle} />
                  </Card>
                </Col>
              </Row>
              <Table
                dataSource={systemRecharges}
                columns={rechargeColumns}
                rowKey="id"
                pagination={listPagination({ size: 'small', style: { marginTop: 6, marginBottom: 0 } })}
                scroll={{ x: 'max-content' }}
                size="small"
                locale={{ emptyText: t('no_system_recharges', '该期间暂无系统钱包明细') }}
              />
            </div>
          )
        },
        {
          key: 'gift',
          label: t('gift_wallet_details', '赠送钱包明细'),
          children: (
            <div>
              <Row gutter={[8, 8]} style={{ marginBottom: 8 }}>
                <Col span={6}>
                  <Card size="small" style={{ background: isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', border: 'none' }}>
                    <Statistic title={<span style={{ color: isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>当前赠送账户余额</span>} value={user.gift_balance || 0} precision={6} prefix={currencySymbol} valueStyle={statisticValueStyle} />
                  </Card>
                </Col>
                <Col span={6}>
                  <Card size="small" style={{ background: isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', border: 'none' }}>
                    <Statistic title={<span style={{ color: isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>{`${timePrefix}获得赠送`}</span>} value={totalGiftRecharge} precision={6} prefix={currencySymbol} valueStyle={statisticValueStyle} />
                  </Card>
                </Col>
                <Col span={6}>
                  <Card size="small" style={{ background: isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', border: 'none' }}>
                    <Statistic title={<span style={{ color: isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>{`${timePrefix}使用赠送`}</span>} value={periodConsumedGift} precision={6} prefix={currencySymbol} valueStyle={statisticValueStyle} loading={consumedLoading} />
                  </Card>
                </Col>
                <Col span={6}>
                  <Card size="small" style={{ background: isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', border: 'none' }}>
                    <Statistic title={<span style={{ color: isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>历史全部赠送使用</span>} value={giftUsed} precision={6} prefix={currencySymbol} valueStyle={statisticValueStyle} />
                  </Card>
                </Col>
              </Row>
              <Table
                dataSource={giftRecharges}
                columns={rechargeColumns}
                rowKey="id"
                pagination={listPagination({ size: 'small', style: { marginTop: 6, marginBottom: 0 } })}
                scroll={{ x: 'max-content' }}
                size="small"
                locale={{ emptyText: t('no_gift_recharges', '该期间暂无赠送钱包明细') }}
              />
            </div>
          )
        },
        {
          key: 'commission',
          label: '佣金钱包明细',
          children: (
            <div>
              <Row gutter={[8, 8]} style={{ marginBottom: 8 }}>
                <Col span={6}>
                  <Card size="small" style={{ background: isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', border: 'none' }}>
                    <Statistic title={<span style={{ color: isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>当前佣金余额</span>} value={commissionBalance} precision={6} prefix={currencySymbol} valueStyle={statisticValueStyle} loading={commissionLoading} />
                  </Card>
                </Col>
                <Col span={6}>
                  <Card size="small" style={{ background: isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', border: 'none' }}>
                    <Statistic title={<span style={{ color: isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>{`${timePrefix}获得`}</span>} value={commissionLedger?.period_earned || 0} precision={6} prefix={currencySymbol} valueStyle={statisticValueStyle} loading={commissionLoading} />
                  </Card>
                </Col>
                <Col span={6}>
                  <Card size="small" style={{ background: isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', border: 'none' }}>
                    <Statistic title={<span style={{ color: isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>{`${timePrefix}回冲`}</span>} value={commissionLedger?.period_clawback || 0} precision={6} prefix={currencySymbol} valueStyle={statisticValueStyle} loading={commissionLoading} />
                  </Card>
                </Col>
                <Col span={6}>
                  <Card size="small" style={{ background: isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', border: 'none' }}>
                    <Statistic title={<span style={{ color: isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>{`${timePrefix}划出`}</span>} value={commissionLedger?.period_transfer || 0} precision={6} prefix={currencySymbol} valueStyle={statisticValueStyle} loading={commissionLoading} />
                  </Card>
                </Col>
              </Row>
              <Table
                dataSource={commissionRows}
                columns={commissionColumns}
                rowKey="id"
                pagination={listPagination({ size: 'small', style: { marginTop: 6, marginBottom: 0 } })}
                scroll={{ x: 'max-content' }}
                size="small"
                loading={commissionLoading}
                locale={{ emptyText: '该期间暂无佣金钱包明细' }}
              />
            </div>
          )
        },
        ...(showCreditTab ? [{
          key: 'credit',
          label: '💳 信控额度明细',
          children: (
            <div>
              <Row gutter={[8, 8]} style={{ marginBottom: 8 }}>
                <Col span={12}>
                  <Card size="small" style={{ background: isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', border: 'none' }}>
                    <Statistic title={<span style={{ color: isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>当前信控额度</span>} value={creditLimit} precision={6} prefix={currencySymbol} valueStyle={getCreditValueStyle()} />
                  </Card>
                </Col>
                <Col span={12}>
                  <Card size="small" style={{ background: isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', border: 'none' }}>
                    <Statistic title={<span style={{ color: isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>{(filterRange && filterRange[0] && filterRange[1]) ? '筛选期内信控调整' : '累计信控调整'}</span>} value={totalCreditRecharge} precision={6} prefix={currencySymbol} valueStyle={statisticValueStyle} />
                  </Card>
                </Col>
              </Row>
              <Table
                dataSource={creditRecharges}
                columns={rechargeColumns}
                rowKey="id"
                pagination={listPagination({ size: 'small', style: { marginTop: 6, marginBottom: 0 } })}
                scroll={{ x: 'max-content' }}
                size="small"
                locale={{ emptyText: '该期间暂无信控额度变更记录' }}
              />
            </div>
          )
        }] : [])
      ]} />
    </div>
  );
};

export default WalletDetailsView;
