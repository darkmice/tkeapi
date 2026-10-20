/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState } from 'react';
import { Button, Tooltip } from 'antd';
import {
  ArrowLeftOutlined,
  CheckCircleFilled,
  ClockCircleOutlined,
  ThunderboltFilled,
} from '@ant-design/icons';
import { useThemeStore } from '../../../store/theme';
import { formatCreditMoney } from '../money';
import PageHeader from '../../../components/PageHeader';
import { planSaleLimitSummary, type StorefrontPlan, useStorefrontPage } from './load';
import '../credits.css';
import '../campaignStorefront.css';

function buyLabel(plan: StorefrontPlan) {
  if (plan.action === 'sold_out') return '已售罄';
  if (plan.action === 'limit') return '已达限购';
  if (plan.action === 'none') return '暂不可购买';
  if (plan.action === 'contact') return '联系我们';
  return '立即抢购';
}

function benefitSpan(mode?: string, days?: number, hours?: number, permanent = '长期有效') {
  if (mode === 'cycle') return '随订阅';
  if (mode === 'hours') return `${hours || 24}小时有效`;
  if (mode === 'days') return `${days || 30}天有效`;
  if (days && days > 0) return `${days}天有效`;
  if (hours && hours > 0) return `${hours}小时有效`;
  return permanent;
}

function planValidityText(plan: StorefrontPlan): string {
  if (plan.credit_enabled === 1) {
    return benefitSpan(plan.credit_duration_mode, plan.credit_duration_days, plan.credit_duration_hours, '长期有效');
  }
  if (plan.space_enabled === 1) {
    return benefitSpan(plan.space_duration_mode, plan.space_duration_days, plan.space_duration_hours, '永久有效');
  }
  if (plan.tier_enabled === 1) {
    return benefitSpan(plan.tier_duration_mode, plan.tier_duration_days, plan.tier_duration_hours, '长期有效');
  }
  return '长期有效';
}

function planLimitText(plan: StorefrontPlan): string {
  const parts: string[] = [];
  if (plan.limit_mode && plan.limit_mode !== 'none' && plan.limit_count) {
    const count = plan.purchased_count ?? 0;
    if (plan.limit_mode === 'lifetime') {
      parts.push(`账号限购 ${count}/${plan.limit_count} 次`);
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
  return parts.length > 0 ? parts.join(' · ') : '不限次数 · 可叠加购买';
}

function primaryFaceValue(plan: StorefrontPlan, symbol: string) {
  if (plan.credit_enabled === 1) {
    const hasModels = (plan.model_ids || []).length > 0;
    const worth = Number(plan.credit_worth || 0);
    const face = Number(plan.credit_amount || 0);
    return {
      caption: hasModels ? '指定模型卡券面值' : '通用卡券面值',
      headline: formatCreditMoney(symbol, face),
      worthText: worth > face ? `面额价值 ${formatCreditMoney(symbol, worth)}` : null,
    };
  }
  if (plan.space_enabled === 1) {
    return {
      caption: '创作空间扩容券',
      headline: `+${plan.space_mb || 0} MB`,
      worthText: null,
    };
  }
  if (plan.tier_enabled === 1 && plan.target_group_key) {
    return {
      caption: '会员等级权益券',
      headline: plan.target_group_key,
      worthText: null,
    };
  }
  return {
    caption: '特惠卡券',
    headline: formatCreditMoney(symbol, plan.price),
    worthText: null,
  };
}

function savingsPillText(plan: StorefrontPlan, symbol: string): string | null {
  if (plan.credit_enabled === 1 && (plan.credit_worth || 0) > (plan.credit_amount || 0)) {
    return `价值 ${formatCreditMoney(symbol, plan.credit_worth || 0)}`;
  }
  if (plan.list_price != null && plan.list_price > plan.price && plan.list_price > 0) {
    const pct = Math.round(((plan.list_price - plan.price) / plan.list_price) * 100);
    if (pct > 0 && pct < 100) {
      return `-${pct}%`;
    }
    const diff = Number((plan.list_price - plan.price).toFixed(2));
    if (diff > 0) return `立省 ${formatCreditMoney(symbol, diff)}`;
  }
  if (plan.credit_enabled === 1 && (plan.credit_amount || 0) > plan.price) {
    const bonus = Number(((plan.credit_amount || 0) - plan.price).toFixed(2));
    if (bonus > 0) return `多得 ${formatCreditMoney(symbol, bonus)}`;
  }
  return null;
}

function extraBenefits(plan: StorefrontPlan, symbol: string): string[] {
  const lines: string[] = [...(plan.features || [])];
  if (plan.credit_enabled === 1) {
    const worth = Number(plan.credit_worth || 0);
    const face = Number(plan.credit_amount || 0);
    if (worth > face) {
      lines.push(`卡券面额价值 ${formatCreditMoney(symbol, worth)}（可消费面值 ${formatCreditMoney(symbol, face)}）`);
    }
    const baseD = Number(plan.credit_discount ?? 1);
    const customEntries = Object.entries(plan.credit_model_discounts || {}).filter(
      ([, v]) => Number.isFinite(Number(v)) && Number(v) > 0 && Number(v) < 1,
    );
    if (baseD > 0 && baseD < 1) {
      lines.push(`有效期内消耗卡券额度享 ${Number((baseD * 10).toFixed(2))} 折优惠`);
    } else if (customEntries.length > 0) {
      lines.push(`有效期内消耗卡券额度享 ${customEntries.length} 个模型专属折扣`);
    }
  }
  if (plan.credit_enabled === 1 && plan.space_enabled === 1 && (plan.space_mb || 0) > 0) {
    const span = benefitSpan(plan.space_duration_mode, plan.space_duration_days, plan.space_duration_hours, '永久有效');
    lines.push(`创作空间 +${plan.space_mb} MB（${span}）`);
  }
  if (plan.space_enabled === 1 && (plan.gift_space_mb || 0) > 0) {
    const giftSpan =
      plan.offer_kind === 'subscription'
        ? '随订阅'
        : benefitSpan(
            plan.gift_space_duration_mode,
            plan.gift_space_duration_days,
            plan.gift_space_duration_hours,
            '永久有效',
          );
    lines.push(`额外赠送扩容 +${plan.gift_space_mb} MB（${giftSpan}）`);
  }
  if (plan.tier_enabled === 1 && plan.target_group_key && (plan.credit_enabled === 1 || plan.space_enabled === 1)) {
    const tierSpan = benefitSpan(plan.tier_duration_mode, plan.tier_duration_days, plan.tier_duration_hours, '长期有效');
    lines.push(`升级会员等级至 ${plan.target_group_key}（${tierSpan}）`);
  }
  return lines;
}

function formatModelZhe(d?: number): string | null {
  const n = Number(d);
  if (!Number.isFinite(n) || n <= 0 || n >= 1) return null;
  return `${Number((n * 10).toFixed(2))}折`;
}

const TkeCouponOffersPage: React.FC = () => {
  const { campaign, missing, leave, buy, symbol, unit, copyright } = useStorefrontPage('tke_coupon');
  const { themeMode } = useThemeStore();
  const [buyingId, setBuyingId] = useState<number | null>(null);

  const packs = (campaign?.plans || []).filter((plan) => plan.offer_kind !== 'subscription');

  const open = (plan: StorefrontPlan) => {
    if (plan.action !== 'buy' || buyingId !== null) return;
    setBuyingId(plan.id);
    window.setTimeout(() => {
      buy(plan);
    }, 320);
  };

  return (
    <div className="order-standalone coupon-storefront">
      <Button className="order-standalone-back" type="text" icon={<ArrowLeftOutlined />} onClick={leave}>
        返回
      </Button>
      {missing && <div className="credits-note">活动不存在或未启用。</div>}
      {campaign && (
        <div className={`credits-page tke-coupon-offers${themeMode === 'light' ? ' light' : ''}`}>
          <PageHeader
            title={campaign.headline || '精选卡券特惠专场'}
            description={
              campaign.subtitle ||
              '购买卡券后自动发放专属消费面值或空间权益，结算时优先抵扣卡券额度，即买即用。'
            }
            marginBottom={campaign.description ? 6 : 12}
            style={{ textAlign: 'center' }}
            titleStyle={{ textAlign: 'center' }}
            descriptionStyle={{ textAlign: 'center' }}
          />
          {campaign.description ? (
            <div className="text-xs text-muted-foreground whitespace-pre-line text-center mb-3 max-w-3xl mx-auto leading-relaxed">
              {campaign.description}
            </div>
          ) : null}

          {packs.length > 0 ? (
            <div className="tke-coupon-grid">
              {packs.map((plan) => {
                const face = primaryFaceValue(plan, symbol);
                const faceLen = face.headline.length;
                const faceFontSize = faceLen >= 11 ? 18 : faceLen >= 9 ? 22 : faceLen >= 8 ? 24 : 26;
                const savings = savingsPillText(plan, symbol);
                const validity = planValidityText(plan);
                const limitText = planLimitText(plan);
                const models = plan.model_ids || [];
                const modelDiscounts = plan.credit_model_discounts || {};
                const discountedModels = Object.keys(modelDiscounts).filter(
                  (m) => formatModelZhe(modelDiscounts[m]) !== null,
                );
                const previewModels = Array.from(
                  new Set([...models.slice(0, 3), ...models.filter((m) => formatModelZhe(modelDiscounts[m]) !== null)]),
                );
                const restModelCount = Math.max(0, models.length - previewModels.length);
                const tooltipModelsText = models
                  .map((m) => {
                    const zhe = formatModelZhe(modelDiscounts[m]);
                    return zhe ? `${m} (${zhe})` : m;
                  })
                  .join('、');
                const extras = extraBenefits(plan, symbol);
                const hasListPrice = plan.list_price != null && plan.list_price > plan.price;

                return (
                  <article
                    key={plan.id}
                    className={`tke-coupon-ticket accent-${plan.accent || 'plain'}${plan.action !== 'buy' ? ' is-disabled' : ''}`}
                  >
                    {/* Left Stub: Face Value & Pricing */}
                    <div className="tke-coupon-stub">
                      <div className="tke-coupon-face-caption">
                        <ThunderboltFilled />
                        <span>{face.caption}</span>
                      </div>
                      <div className="tke-coupon-face-value" style={{ fontSize: faceFontSize }}>
                        {face.headline}
                      </div>
                      {savings && (
                        <div className="tke-coupon-savings-pill">
                          <span>{savings}</span>
                        </div>
                      )}
                      <div className="tke-coupon-price-box">
                        {hasListPrice && (
                          <div className="tke-coupon-list-price">
                            原价 {formatCreditMoney(symbol, plan.list_price || 0)}
                          </div>
                        )}
                        <div className="tke-coupon-sale-price">
                          <span className="tke-coupon-sale-label">实付</span>
                          <span className="tke-coupon-sale-amount">{formatCreditMoney(symbol, plan.price)}</span>
                          {unit ? <span className="tke-coupon-sale-unit">{unit}</span> : null}
                        </div>
                      </div>
                    </div>

                    {/* Perforation Divider with Top/Bottom Notches */}
                    <div className="tke-coupon-divider" aria-hidden="true">
                      <span className="tke-coupon-notch tke-coupon-notch-top" />
                      <span className="tke-coupon-perforation" />
                      <span className="tke-coupon-notch tke-coupon-notch-bottom" />
                    </div>

                    {/* Right Body: Details, Model Scope & CTA */}
                    <div className="tke-coupon-body">
                      <div className="tke-coupon-main">
                        <div className="tke-coupon-title-row">
                          <div className="tke-coupon-title-group">
                            <h3 className="tke-coupon-name">{plan.name}</h3>
                            {plan.badge ? <span className="tke-coupon-badge">{plan.badge}</span> : null}
                          </div>
                          <span className="tke-coupon-validity">
                            <ClockCircleOutlined />
                            <span>{validity}</span>
                          </span>
                        </div>

                        {(plan.subtitle || plan.description) && (
                          <div className="tke-coupon-subtitle">{plan.subtitle || plan.description}</div>
                        )}

                        {plan.credit_enabled === 1 && (
                          <div className="tke-coupon-scope">
                            <span className="tke-coupon-scope-label">适用模型</span>
                            {models.length === 0 ? (
                              <div className="tke-coupon-model-list">
                                <span className="tke-coupon-model-tag is-all">全平台模型通用</span>
                                {discountedModels.map((modelId) => {
                                  const zhe = formatModelZhe(modelDiscounts[modelId]);
                                  return (
                                    <span key={modelId} className="tke-coupon-model-tag">
                                      <span>{modelId}</span>
                                      {zhe && <span className="tke-coupon-model-zhe">{zhe}</span>}
                                    </span>
                                  );
                                })}
                              </div>
                            ) : (
                              <div className="tke-coupon-model-list">
                                {previewModels.map((modelId) => {
                                  const zhe = formatModelZhe(modelDiscounts[modelId]);
                                  return (
                                    <span key={modelId} className="tke-coupon-model-tag">
                                      <span>{modelId}</span>
                                      {zhe && <span className="tke-coupon-model-zhe">{zhe}</span>}
                                    </span>
                                  );
                                })}
                                {restModelCount > 0 && (
                                  <Tooltip title={tooltipModelsText}>
                                    <span className="tke-coupon-model-tag is-more">+{restModelCount} 个模型</span>
                                  </Tooltip>
                                )}
                              </div>
                            )}
                          </div>
                        )}

                        {extras.length > 0 && (
                          <ul className="tke-coupon-benefits">
                            {extras.map((line) => (
                              <li key={line}>
                                <CheckCircleFilled />
                                <span>{line}</span>
                              </li>
                            ))}
                          </ul>
                        )}
                      </div>

                      <div className="tke-coupon-footer">
                        <span className="tke-coupon-limit">{limitText}</span>
                        <Button
                          type="primary"
                          className="tke-coupon-buy-btn"
                          loading={buyingId === plan.id}
                          disabled={plan.action !== 'buy' || (buyingId !== null && buyingId !== plan.id)}
                          onClick={() => open(plan)}
                        >
                          {buyLabel(plan)}
                        </Button>
                      </div>
                    </div>
                  </article>
                );
              })}
            </div>
          ) : (
            <div className="credits-note">当前活动暂无可售卖的卡券方案。</div>
          )}
        </div>
      )}
      {copyright ? <div className="coupon-storefront-copyright">{copyright}</div> : null}
    </div>
  );
};

export default TkeCouponOffersPage;
