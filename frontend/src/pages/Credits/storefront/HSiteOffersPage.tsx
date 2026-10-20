/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React from 'react';
import { Button } from 'antd';
import { ArrowLeftOutlined } from '@ant-design/icons';
import { formatCreditMoney } from '../money';
import { type StorefrontPlan, useStorefrontPage } from './load';
import '../credits.css';
import '../campaignStorefront.css';

const TONES = ['starter', 'plus', 'ultra', 'teams'];

function cycleLines(plan: StorefrontPlan) {
  const renewing = !!plan.can_renew;
  const rows: { key: string; label: string; price: number }[] = [];
  const push = (key: string, label: string, sale?: number | null, renew?: number | null) => {
    if (sale == null) return;
    rows.push({ key, label, price: renewing && renew != null ? renew : sale });
  };
  push('month', ' / 月', plan.price_month, plan.renew_price_month);
  push('quarter', ' / 季', plan.price_quarter, plan.renew_price_quarter);
  push('year', ' / 年', plan.price_year, plan.renew_price_year);
  return rows;
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

const HSiteOffersPage: React.FC = () => {
  const { campaign, missing, leave, buy, symbol, copyright } = useStorefrontPage('h_site');
  const plans = campaign?.plans || [];

  return (
    <div className="order-standalone coupon-storefront is-dark">
      <Button className="order-standalone-back" type="text" icon={<ArrowLeftOutlined />} onClick={leave}>
        返回
      </Button>
      {missing && <div className="credits-note">活动不存在或未启用。</div>}
      {campaign && (
        <div className="pg-ig-hplan">
          <h2 className="pg-ig-hplan-title">{campaign.headline || '热门活动'}</h2>
          {campaign.subtitle ? <p className="pg-ig-hplan-sub">{campaign.subtitle}</p> : null}
          {campaign.description ? (
            <p className="pg-ig-hplan-sub" style={{ whiteSpace: 'pre-line', opacity: 0.8, marginTop: 4 }}>
              {campaign.description}
            </p>
          ) : null}
          <div className="pg-ig-hplan-grid">
            {plans.map((plan, index) => (
              <article key={plan.id} className={`pg-ig-hplan-card is-${TONES[index % TONES.length]}`}>
                <div className="pg-ig-hplan-head">
                  <div className="pg-ig-hplan-name">{plan.name}</div>
                  {plan.badge ? <span className="pg-ig-hplan-badge">{plan.badge}</span> : null}
                </div>
                <div className="pg-ig-hplan-desc">{plan.subtitle || plan.description}</div>
                {(plan.features?.length || (plan.credit_enabled === 1 && plan.model_ids?.length)) ? (
                  <ul className="pg-ig-hplan-features">
                    {(plan.features || []).map((line) => <li key={line}>{line}</li>)}
                    {plan.credit_enabled === 1 && plan.model_ids?.length ? (
                      <li>适用模型：{plan.model_ids.join('、')}</li>
                    ) : null}
                  </ul>
                ) : null}
                <div className="pg-ig-hplan-price">
                  {cycleLines(plan).length
                    ? cycleLines(plan).map((item) => <div key={item.key}>{formatCreditMoney(symbol, item.price)}{item.label}</div>)
                    : formatCreditMoney(symbol, plan.price)}
                </div>
                <button type="button" className="pg-ig-hplan-buy" disabled={plan.action !== 'buy'} onClick={() => buy(plan)}>
                  {buyLabel(plan)}
                </button>
              </article>
            ))}
          </div>
          {!plans.length && <div className="credits-note">这里还没有可展示的卡券方案。</div>}
        </div>
      )}
      {copyright ? <div className="coupon-storefront-copyright">{copyright}</div> : null}
    </div>
  );
};

export default HSiteOffersPage;
