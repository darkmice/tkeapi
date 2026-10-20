/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useEffect, useState } from 'react';
import { Button, Typography } from 'antd';
import { ArrowLeftOutlined, CheckOutlined } from '@ant-design/icons';
import { useNavigate, useLocation } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import request from '../../utils/request';
import { couponCampaignPath, offerReturnFromState, rememberedOfferReturn } from './storefront/path';
import useSettingsStore from '../../store/settings';
import { useThemeStore } from '../../store/theme';
import { formatCreditMoney } from './money';
import PageHeader from '../../components/PageHeader';
import './credits.css';

const { Title, Paragraph, Text } = Typography;

type Plan = {
  id: number;
  name: string;
  subtitle: string;
  description: string;
  price: number;
  card_role: string;
  accent: string;
  badge: string;
  contact_url: string;
  features: string[];
  action: string;
  is_current: boolean;
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
  credit_duration_mode: string;
  credit_duration_days: number;
  credit_duration_hours: number;
  model_ids: string[];
  limit_mode: string;
  limit_count: number;
  limit_window_days: number;
  limit_window_hours: number;
  sale_limit_total?: number;
  sale_limit_daily?: number;
  sale_limit_window_days?: number;
  sale_limit_window_count?: number;
  sold_total_count?: number;
  sold_daily_count?: number;
  sold_window_count?: number;
  space_enabled: number;
  space_mb: number;
  space_duration_mode: string;
  space_duration_days: number;
  space_duration_hours: number;
  gift_space_mb?: number;
  gift_space_duration_mode?: string;
  gift_space_duration_days?: number;
  gift_space_duration_hours?: number;
  purchased_count: number;
  offer_kind?: string;
  billing_cycle?: string;
  can_renew?: boolean;
  content_changed?: boolean;
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
};

const CreditsTiers: React.FC = () => {
  const { i18n } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();
  const en = (i18n.language || '').toLowerCase().startsWith('en');
  const { settings } = useSettingsStore();
  const { themeMode } = useThemeStore();
  const symbol = settings?.currency?.currency_symbol || '$';
  const unit = settings?.currency?.currency_unit || '';
  const [plans, setPlans] = useState<Plan[]>([]);
  const [pageTitle, setPageTitle] = useState('');
  const [pageSubtitle, setPageSubtitle] = useState('');
  const [levelName, setLevelName] = useState('');
  const [balance, setBalance] = useState(0);
  const [buyingId, setBuyingId] = useState<number | null>(null);

  const loadCatalog = async () => {
    const [placed, catalog] = await Promise.all([
      request.get('/plugins/marketing_membership/placements/console_nav_coupon') as Promise<{
        campaigns?: Array<{ id?: number; template_key?: string; headline?: string; subtitle?: string; plans?: Plan[] }>;
      }>,
      request.get('/plugins/marketing_membership/catalog') as Promise<{
        level_name: string;
        balance: number;
      }>,
    ]);
    const campaigns = placed.campaigns || [];
    const target = campaigns.find((row) => row?.id);
    if (target?.id) {
      const from = offerReturnFromState(location.state) || rememberedOfferReturn();
      navigate(couponCampaignPath(target.template_key, target.id), {
        replace: true,
        state: from ? { from } : undefined,
      });
      return;
    }
    const seen = new Set<number>();
    const next: Plan[] = [];
    for (const campaign of campaigns) {
      for (const plan of campaign.plans || []) {
        if (seen.has(plan.id)) continue;
        seen.add(plan.id);
        next.push(plan);
      }
    }
    setPlans(next);
    setPageTitle(campaigns.length === 1 ? (campaigns[0].headline || '') : '');
    setPageSubtitle(campaigns.length === 1 ? (campaigns[0].subtitle || '') : '');
    setLevelName(catalog.level_name || '');
    setBalance(catalog.balance || 0);
  };

  useEffect(() => {
    loadCatalog();
  }, []);

  const cards = plans.filter((plan) => plan.card_role !== 'banner');
  const banners = plans.filter((plan) => plan.card_role === 'banner');

  const buttonLabel = (plan: Plan) => {
    if (plan.action === 'current') return en ? 'Current plan' : '当前方案';
    if (plan.action === 'sold_out') return en ? 'Sold out' : '已售罄';
    if (plan.action === 'limit') return en ? 'Limit reached' : '已达购买上限';
    if (plan.action === 'none') return en ? 'Unavailable' : '不可购买';
    if (plan.action === 'contact') return en ? 'Get in touch' : '联系我们';
    if (plan.can_renew && plan.content_changed) return en ? `Renew updated ${plan.name}` : `新方案续费 ${plan.name}`;
    if (plan.can_renew) return en ? `Renew ${plan.name}` : `续费 ${plan.name}`;
    return en ? `Choose ${plan.name}` : `选择 ${plan.name}`;
  };

  const pricePeriod = (plan: Plan) => {
    if (plan.card_role === 'payg') return en ? ' / no charge' : ' / 无月费';
    return en ? ' / once' : ' / 一次性';
  };

  const subscriptionPrices = (plan: Plan) => {
    const rows: { key: string; label: string; price: number; list?: number | null; renew?: number | null }[] = [];
    const renewing = !!plan.can_renew;
    const push = (
      key: string,
      label: string,
      sale?: number | null,
      list?: number | null,
      renew?: number | null,
    ) => {
      if (sale == null) return;
      const price = renewing && renew != null ? renew : sale;
      rows.push({
        key,
        label,
        price,
        list: list != null && list > price ? list : null,
        renew: !renewing && renew != null && renew !== sale ? renew : null,
      });
    };
    push('month', en ? '/ month' : '/ 月', plan.price_month, plan.list_price_month, plan.renew_price_month);
    push('quarter', en ? '/ quarter' : '/ 季', plan.price_quarter, plan.list_price_quarter, plan.renew_price_quarter);
    push('year', en ? '/ year' : '/ 年', plan.price_year, plan.list_price_year, plan.renew_price_year);
    return rows;
  };

  const durationText = (mode: string, days: number, hours = 0, permanent = en ? 'no expiry' : '长期') => {
    if (mode === 'cycle') return en ? 'for the purchased cycle' : '按购买周期';
    if (mode === 'hours') return en ? `${hours || 24} hours` : `${hours || 24} 小时`;
    if (mode === 'days') return en ? `${days || 30} days` : `${days || 30} 天`;
    if (days && days > 0) return en ? `${days} days` : `${days} 天`;
    if (hours && hours > 0) return en ? `${hours} hours` : `${hours} 小时`;
    return permanent;
  };

  const windowLimitText = (plan: Plan) => {
    const hours = plan.limit_window_hours || 0;
    const days = plan.limit_window_days || 0;
    const span = hours > 0
      ? (en ? `${hours} hours` : `${hours}小时`)
      : (en ? `${days} days` : `${days}天`);
    return en
      ? `${plan.purchased_count}/${plan.limit_count} within ${span}`
      : `${span}内 ${plan.purchased_count}/${plan.limit_count}`;
  };

  const leave = () => {
    if (window.history.length > 1) {
      navigate(-1);
      return;
    }
    navigate('/dashboard');
  };

  return (
    <div className="order-standalone">
    <Button className="order-standalone-back" type="text" icon={<ArrowLeftOutlined />} onClick={leave}>
      {en ? 'Back' : '返回'}
    </Button>
    <div className={`credits-page${themeMode === 'light' ? ' light' : ''}`}>
      <PageHeader
        title={pageTitle || (en ? 'Coupon subscription' : '卡券订阅')}
        description={
          pageSubtitle
            || (en
              ? 'One card can include model credit, a membership tier, extra Playground 2026 storage, or any combination.'
              : '一张卡可以包含消费额度、会员等级、创作中心 2026 空间，也可以只含其中一项。')
        }
        extra={
          <div className="flex items-center gap-2 text-sm text-muted-foreground font-mono">
            <span>{en ? 'Tier' : '当前等级'}：{levelName || '—'}</span>
            <span>·</span>
            <span>{en ? 'Balance' : '余额'}：{formatCreditMoney(symbol, balance)} {unit}</span>
          </div>
        }
        marginBottom={24}
      />
      <div className="credits-grid">
        {cards.map((plan) => (
          <article key={plan.id} className={`credits-card accent-${plan.accent || 'plain'}`}>
            {plan.accent !== 'plain' && <div className="credits-art" />}
            <div className="credits-card-body">
              <div className="credits-name">{plan.name}</div>
              <div className="credits-sub">{plan.subtitle || plan.description}</div>
              {plan.offer_kind === 'subscription' && subscriptionPrices(plan).length > 0 ? (
                <div className="credits-price" style={{ fontSize: 22, lineHeight: 1.45 }}>
                  {subscriptionPrices(plan).map((item) => (
                    <div key={item.key}>
                      {item.list != null && item.list > item.price ? (
                        <span style={{ textDecoration: 'line-through', marginRight: 8 }}>{formatCreditMoney(symbol, item.list)}</span>
                      ) : null}
                      {formatCreditMoney(symbol, item.price)}
                      <span>{unit}{item.label}</span>
                      {item.renew != null ? (
                        <div style={{ fontSize: 12, fontWeight: 500, opacity: 0.75 }}>
                          {en ? 'Renew ' : '续费 '}{formatCreditMoney(symbol, item.renew)}
                        </div>
                      ) : null}
                    </div>
                  ))}
                </div>
              ) : (
                <div className="credits-price">
                  {plan.list_price != null && plan.list_price > plan.price ? (
                    <span style={{ textDecoration: 'line-through', marginRight: 8 }}>{formatCreditMoney(symbol, plan.list_price)}</span>
                  ) : null}
                  {formatCreditMoney(symbol, plan.price)}
                  <span>{unit}{plan.offer_kind === 'subscription' ? (en ? ' / month' : ' / 月') : pricePeriod(plan)}</span>
                </div>
              )}
              {plan.badge ? <Text style={{ color: 'inherit', opacity: 0.85 }}>{plan.badge}</Text> : null}
              <Button
                block
                loading={buyingId === plan.id}
                disabled={(plan.action !== 'buy' && plan.action !== 'contact') || (buyingId !== null && buyingId !== plan.id)}
                onClick={() => {
                  if (plan.action === 'contact' && plan.contact_url) {
                    window.open(plan.contact_url, '_blank', 'noopener,noreferrer');
                    return;
                  }
                  if (plan.action === 'buy' && buyingId === null) {
                    setBuyingId(plan.id);
                    window.setTimeout(() => {
                      navigate(`/orders/checkout/${plan.id}`);
                    }, 320);
                  }
                }}
              >
                {buttonLabel(plan)}
              </Button>
              <div className="credits-includes">{en ? 'Plan includes' : '方案包含'}</div>
              <ul className="credits-features">
                {(plan.features || []).map((line) => (
                  <li key={line}><CheckOutlined /> <span>{line}</span></li>
                ))}
                {plan.tier_enabled === 1 && (
                  <li>
                    <CheckOutlined />
                    <span>
                      {en ? `Tier ${plan.target_group_key}` : `升级到 ${plan.target_group_key}`}
                      {' · '}
                      {durationText(plan.tier_duration_mode, plan.tier_duration_days, plan.tier_duration_hours)}
                    </span>
                  </li>
                )}
                {plan.credit_enabled === 1 && (
                  <li>
                    <CheckOutlined />
                    <span>
                      {en ? 'Credit' : '消费额度'} {formatCreditMoney(symbol, plan.credit_amount)}
                      {(plan.credit_worth || 0) > plan.credit_amount
                        ? (en
                          ? ` (Worth ${formatCreditMoney(symbol, plan.credit_worth || 0)})`
                          : `（价值 ${formatCreditMoney(symbol, plan.credit_worth || 0)}）`)
                        : ''}
                      {' · '}
                      {durationText(plan.credit_duration_mode, plan.credit_duration_days, plan.credit_duration_hours)}
                      {plan.model_ids?.length ? ` · ${en ? 'Models: ' : '适用模型：'}${plan.model_ids.join(en ? ', ' : '、')}` : (en ? ' · all models' : ' · 全部模型')}
                      {plan.credit_discount != null && plan.credit_discount > 0 && plan.credit_discount < 1
                        ? (en
                          ? ` · ${Math.round(plan.credit_discount * 100)}% rate`
                          : ` · 享 ${Number((plan.credit_discount * 10).toFixed(2))} 折`)
                        : ''}
                    </span>
                  </li>
                )}
                {plan.space_enabled === 1 && (
                  <li>
                    <CheckOutlined />
                    <span>
                      {en ? `+${plan.space_mb} MB storage` : `创作空间 +${plan.space_mb} MB`}
                      {' · '}
                      {plan.offer_kind === 'subscription'
                        ? (en ? 'with subscription' : '随订阅')
                        : durationText(plan.space_duration_mode || 'permanent', plan.space_duration_days || 0, plan.space_duration_hours || 0, en ? 'permanent' : '永久')}
                    </span>
                  </li>
                )}
                {(plan.gift_space_mb || 0) > 0 && (
                  <li>
                    <CheckOutlined />
                    <span>
                      {en ? `Gift +${plan.gift_space_mb} MB` : `赠送扩容 +${plan.gift_space_mb} MB`}
                      {' · '}
                      {plan.offer_kind === 'subscription'
                        ? (en ? 'with subscription' : '随订阅')
                        : durationText(plan.gift_space_duration_mode || 'permanent', plan.gift_space_duration_days || 0, plan.gift_space_duration_hours || 0, en ? 'permanent' : '永久')}
                    </span>
                  </li>
                )}
                {plan.limit_mode === 'lifetime' && (
                  <li><CheckOutlined /><span>{en ? `${plan.purchased_count}/${plan.limit_count} lifetime` : `永久限购 ${plan.purchased_count}/${plan.limit_count}`}</span></li>
                )}
                {plan.limit_mode === 'window' && (
                  <li><CheckOutlined /><span>{windowLimitText(plan)}</span></li>
                )}
                {(plan.sale_limit_daily || 0) > 0 && (
                  <li><CheckOutlined /><span>{en ? `Daily quota ${plan.sold_daily_count ?? 0}/${plan.sale_limit_daily}` : `今日限量 ${plan.sold_daily_count ?? 0}/${plan.sale_limit_daily} 份`}</span></li>
                )}
                {(plan.sale_limit_window_days || 0) > 0 && (plan.sale_limit_window_count || 0) > 0 && (
                  <li><CheckOutlined /><span>{en ? `${plan.sale_limit_window_days}-day quota ${plan.sold_window_count ?? 0}/${plan.sale_limit_window_count}` : `${plan.sale_limit_window_days}天限量 ${plan.sold_window_count ?? 0}/${plan.sale_limit_window_count} 份`}</span></li>
                )}
                {(plan.sale_limit_total || 0) > 0 && (
                  <li><CheckOutlined /><span>{en ? `Total quota ${plan.sold_total_count ?? 0}/${plan.sale_limit_total}` : `总限量 ${plan.sold_total_count ?? 0}/${plan.sale_limit_total} 份`}</span></li>
                )}
              </ul>
            </div>
          </article>
        ))}
      </div>

      {banners.map((plan) => (
        <div key={plan.id} className="credits-banner">
          <div>
            <div className="credits-name">{plan.name}</div>
            <div>{plan.subtitle || plan.description}</div>
          </div>
          <Button
            disabled={!plan.contact_url}
            onClick={() => plan.contact_url && window.open(plan.contact_url, '_blank', 'noopener,noreferrer')}
          >
            {en ? 'Get in touch' : '联系我们'}
          </Button>
        </div>
      ))}

      {!cards.length && (
        <div className="credits-note">
          {en ? 'No coupon campaign is on this page yet.' : '这里还没有可展示的卡券活动。'}
        </div>
      )}

      <div className="credits-note">
        {en
          ? 'Credit pays eligible model usage and is not refunded as cash. The default tier changes immediately. A non-default tier changes only when that tier is listed as upgradable. Extra storage adds to the tier quota you already have, and stacks when you buy again. Purchase limits are per account.'
          : '消费额度只抵扣允许的模型消费，退款退回卡券，不会退成现金。默认等级购买后立即变更；非默认等级只有配置为可升级时才会变更。创作空间加在当前等级已有配额上，重复购买会叠加。购买次数按账号限制。'}
      </div>

    </div>
    </div>
  );
};

export default CreditsTiers;
