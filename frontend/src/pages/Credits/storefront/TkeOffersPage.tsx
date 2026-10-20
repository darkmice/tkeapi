/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useEffect, useState } from 'react';
import { Button, Segmented, Typography } from 'antd';
import { ArrowLeftOutlined, CheckOutlined } from '@ant-design/icons';
import { useNavigate } from 'react-router-dom';
import { useThemeStore } from '../../../store/theme';
import { formatCreditMoney } from '../money';
import PageHeader from '../../../components/PageHeader';
import { planSaleLimitSummary, type StorefrontPlan, useStorefrontPage } from './load';
import '../credits.css';
import '../campaignStorefront.css';

const { Text } = Typography;

type Cycle = 'month' | 'quarter' | 'year';

const CYCLE_LABEL: Record<Cycle, string> = {
  month: '按月',
  quarter: '按季度',
  year: '按年',
};

const CYCLE_UNIT: Record<Cycle, string> = {
  month: ' / 月',
  quarter: ' / 季',
  year: ' / 年',
};

function cycleOffer(plan: StorefrontPlan, cycle: Cycle) {
  const sale = cycle === 'month' ? plan.price_month : cycle === 'quarter' ? plan.price_quarter : plan.price_year;
  const list = cycle === 'month' ? plan.list_price_month : cycle === 'quarter' ? plan.list_price_quarter : plan.list_price_year;
  const renew = cycle === 'month' ? plan.renew_price_month : cycle === 'quarter' ? plan.renew_price_quarter : plan.renew_price_year;
  if (sale == null) return null;
  const price = plan.can_renew && renew != null ? renew : sale;
  return {
    price,
    list: list != null && list > price ? list : null,
    renew: !plan.can_renew && renew != null && renew !== sale ? renew : null,
  };
}

function buyLabel(plan: StorefrontPlan) {
  if (plan.action === 'sold_out') return '已售罄';
  if (plan.action === 'limit') return '已达限购';
  if (plan.action === 'none') return '暂不可购买';
  if (plan.action === 'contact') return '联系我们';
  if (plan.can_renew && plan.content_changed) return '新方案续费';
  if (plan.can_renew) return '立即续费';
  return '立即购买';
}

function benefitSpan(mode?: string, days?: number, hours?: number, permanent = '永久') {
  if (mode === 'cycle') return '随订阅';
  if (mode === 'hours') return `${hours || 24}小时`;
  if (mode === 'days') return `${days || 30}天`;
  if (days && days > 0) return `${days}天`;
  if (hours && hours > 0) return `${hours}小时`;
  return permanent;
}

function planLimitText(plan: StorefrontPlan): string | null {
  const parts: string[] = [];
  if (plan.limit_mode && plan.limit_mode !== 'none' && plan.limit_count) {
    const count = plan.purchased_count ?? 0;
    if (plan.limit_mode === 'lifetime') {
      parts.push(`永久限购 ${count}/${plan.limit_count} 次`);
    } else if (plan.limit_mode === 'window') {
      const hours = plan.limit_window_hours || 0;
      const days = plan.limit_window_days || 0;
      const span = hours > 0 ? `${hours}小时` : `${days}天`;
      parts.push(`${span}内限购 ${count}/${plan.limit_count} 次`);
    }
  }
  const saleSummary = planSaleLimitSummary(plan);
  if (saleSummary) {
    parts.push(saleSummary);
  }
  return parts.length > 0 ? parts.join(' · ') : null;
}

function formatPlanCreditLine(plan: StorefrontPlan, symbol: string): string {
  const face = Number(plan.credit_amount || 0);
  const worth = Number(plan.credit_worth || 0);
  const baseD = Number(plan.credit_discount ?? 1);
  const customCount = Object.values(plan.credit_model_discounts || {}).filter(
    (v) => Number.isFinite(Number(v)) && Number(v) > 0 && Number(v) < 1,
  ).length;
  const worthPart = worth > face ? `（面额价值 ${formatCreditMoney(symbol, worth)}）` : '';
  const models = plan.model_ids || [];
  const modelPart = models.length > 0 ? ` · 适用模型：${models.join('、')}` : ' · 全模型通用';
  const discountPart =
    baseD > 0 && baseD < 1
      ? ` · 额度享 ${Number((baseD * 10).toFixed(2))} 折`
      : customCount > 0
        ? ` · ${customCount} 模型享专属折扣`
        : '';
  return `消费额度 ${formatCreditMoney(symbol, face)}${worthPart}${modelPart}${discountPart}`;
}

function packLines(plan: StorefrontPlan, symbol: string) {
  const lines = [...(plan.features || [])];
  if (plan.tier_enabled === 1 && plan.target_group_key) lines.push(`升级到 ${plan.target_group_key}`);
  if (plan.credit_enabled === 1) lines.push(formatPlanCreditLine(plan, symbol));
  if (plan.space_enabled === 1) {
    const span = benefitSpan(plan.space_duration_mode, plan.space_duration_days, plan.space_duration_hours, '永久');
    lines.push(`创作空间 +${plan.space_mb} MB · ${span}`);
    if ((plan.gift_space_mb || 0) > 0) {
      const gift = benefitSpan(plan.gift_space_duration_mode, plan.gift_space_duration_days, plan.gift_space_duration_hours, '永久');
      lines.push(`赠送扩容 +${plan.gift_space_mb} MB · ${gift}`);
    }
  }
  const limit = planLimitText(plan);
  if (limit) lines.push(limit);
  return lines;
}

const TkeOffersPage: React.FC = () => {
  const navigate = useNavigate();
  const { campaign, missing, leave, buy, symbol, unit, copyright } = useStorefrontPage('tke');
  const { themeMode } = useThemeStore();
  const plans = campaign?.plans || [];
  const subscriptions = plans.filter((plan) => plan.offer_kind === 'subscription');
  const packs = plans.filter((plan) => plan.offer_kind !== 'subscription');
  const cycles = (['month', 'quarter', 'year'] as Cycle[]).filter((cycle) =>
    subscriptions.some((plan) => cycleOffer(plan, cycle)),
  );
  const [cycle, setCycle] = useState<Cycle>('month');
  const [buyingId, setBuyingId] = useState<number | null>(null);

  const cycleKey = cycles.join(',');
  useEffect(() => {
    if (!cycleKey) return;
    const next = cycleKey.split(',') as Cycle[];
    if (!next.includes(cycle)) setCycle(next[0]);
  }, [cycleKey, cycle]);

  const open = (plan: StorefrontPlan) => {
    if (plan.action !== 'buy' || buyingId !== null) return;
    setBuyingId(plan.id);
    window.setTimeout(() => {
      if (plan.offer_kind === 'subscription') {
        navigate(`/orders/checkout/${plan.id}?cycle=${cycle}`);
        return;
      }
      buy(plan);
    }, 320);
  };

  const visibleSubs = subscriptions.filter((plan) => cycleOffer(plan, cycle));

  return (
    <div className="order-standalone coupon-storefront">
      <Button className="order-standalone-back" type="text" icon={<ArrowLeftOutlined />} onClick={leave}>
        返回
      </Button>
      {missing && <div className="credits-note">活动不存在或未启用。</div>}
      {campaign && (
        <div className={`credits-page tke-offers${themeMode === 'light' ? ' light' : ''}`}>
          <PageHeader
            title={campaign.headline || '卡券订阅'}
            description={campaign.subtitle || '一张卡可以包含消费额度、会员等级、创作中心 2026 空间，也可以只含其中一项。'}
            marginBottom={campaign.description ? 6 : 8}
          />
          {campaign.description ? (
            <div className="text-xs text-muted-foreground whitespace-pre-line mb-3 leading-relaxed">
              {campaign.description}
            </div>
          ) : null}

          {subscriptions.length > 0 && (
            <section className="tke-block">
              <div className="tke-block-title">订阅</div>
              {cycles.length > 0 && (
                <div className="tke-cycle-switch">
                  <Segmented
                    value={cycle}
                    onChange={(value) => setCycle(value as Cycle)}
                    options={cycles.map((item) => ({ label: CYCLE_LABEL[item], value: item }))}
                  />
                </div>
              )}
              <div className="tke-sub-row">
                {visibleSubs.map((plan) => {
                  const offer = cycleOffer(plan, cycle);
                  return (
                    <article key={plan.id} className={`credits-card accent-${plan.accent || 'plain'}`}>
                      {plan.accent && plan.accent !== 'plain' && <div className="credits-art" />}
                      <div className="credits-card-body">
                        <div className="credits-name">{plan.name}</div>
                        <div className="credits-sub">{plan.subtitle || plan.description}</div>
                        <div className="credits-price">
                          {offer?.list != null && (
                            <span style={{ textDecoration: 'line-through', marginRight: 8, fontSize: 16 }}>{formatCreditMoney(symbol, offer.list)}</span>
                          )}
                          {formatCreditMoney(symbol, offer?.price ?? plan.price)}
                          <span>{unit}{CYCLE_UNIT[cycle]}</span>
                          {offer?.renew != null && (
                            <div style={{ fontSize: 12, fontWeight: 500, opacity: 0.75, marginTop: 6 }}>续费 {formatCreditMoney(symbol, offer.renew)}</div>
                          )}
                        </div>
                        {plan.badge ? <Text style={{ color: 'inherit', opacity: 0.85 }}>{plan.badge}</Text> : null}
                        <Button
                          block
                          loading={buyingId === plan.id}
                          disabled={plan.action !== 'buy' || (buyingId !== null && buyingId !== plan.id)}
                          onClick={() => open(plan)}
                        >
                          {buyLabel(plan)}
                        </Button>
                        <div className="credits-includes">方案包含</div>
                        <ul className="credits-features">
                          {(plan.features || []).map((line) => (
                            <li key={line}><CheckOutlined /> <span>{line}</span></li>
                          ))}
                          {plan.tier_enabled === 1 && <li><CheckOutlined /><span>升级到 {plan.target_group_key}</span></li>}
                          {plan.credit_enabled === 1 && <li><CheckOutlined /><span>{formatPlanCreditLine(plan, symbol)}</span></li>}
                          {plan.space_enabled === 1 && (
                            <li>
                              <CheckOutlined />
                              <span>
                                创作空间 +{plan.space_mb} MB · {benefitSpan(plan.space_duration_mode, plan.space_duration_days, plan.space_duration_hours, '随订阅')}
                              </span>
                            </li>
                          )}
                          {(plan.gift_space_mb || 0) > 0 && (
                            <li>
                              <CheckOutlined />
                              <span>
                                赠送扩容 +{plan.gift_space_mb} MB · {benefitSpan(plan.gift_space_duration_mode, plan.gift_space_duration_days, plan.gift_space_duration_hours, '随订阅')}
                              </span>
                            </li>
                          )}
                          {planLimitText(plan) && (
                            <li>
                              <CheckOutlined />
                              <span>{planLimitText(plan)}</span>
                            </li>
                          )}
                        </ul>
                      </div>
                    </article>
                  );
                })}
              </div>
              {!visibleSubs.length && <div className="credits-note">这个周期没有在售的订阅。</div>}
            </section>
          )}

          {packs.length > 0 && (
            <section className="tke-block">
              <div className="tke-block-title">加油包</div>
              <div className="tke-pack-list">
                {packs.map((plan) => (
                  <article key={plan.id} className={`tke-pack accent-${plan.accent || 'plain'}`}>
                    {plan.badge ? <div className="tke-pack-badge">{plan.badge}</div> : null}
                    <div className="tke-pack-stub">
                      {plan.list_price != null && plan.list_price > plan.price && (
                        <div className="tke-pack-list-price">{formatCreditMoney(symbol, plan.list_price)}</div>
                      )}
                      <div className="tke-pack-price">{formatCreditMoney(symbol, plan.price)}</div>
                    </div>
                    <div className="tke-pack-main">
                      <div className="tke-pack-copy">
                        <div className="tke-pack-name">{plan.name}</div>
                        {(plan.subtitle || plan.description) && (
                          <div className="tke-pack-sub">{plan.subtitle || plan.description}</div>
                        )}
                        {packLines(plan, symbol).length > 0 && (
                          <ul className="tke-pack-features">
                            {packLines(plan, symbol).map((line) => <li key={line}>{line}</li>)}
                          </ul>
                        )}
                      </div>
                      <Button
                        loading={buyingId === plan.id}
                        disabled={plan.action !== 'buy' || (buyingId !== null && buyingId !== plan.id)}
                        onClick={() => open(plan)}
                      >
                        {buyLabel(plan)}
                      </Button>
                    </div>
                  </article>
                ))}
              </div>
            </section>
          )}

          {!plans.length && <div className="credits-note">这里还没有可展示的卡券方案。</div>}
        </div>
      )}
      {copyright ? <div className="coupon-storefront-copyright">{copyright}</div> : null}
    </div>
  );
};

export default TkeOffersPage;
