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

function shownPrice(plan: StorefrontPlan): { price: number; unit: string } {
  if (plan.offer_kind !== 'subscription') {
    return { price: plan.price, unit: '一次性' };
  }
  const renewing = !!plan.can_renew;
  const cycles: Array<[number | null | undefined, number | null | undefined, string]> = [
    [plan.price_month, plan.renew_price_month, '月'],
    [plan.price_quarter, plan.renew_price_quarter, '季'],
    [plan.price_year, plan.renew_price_year, '年'],
  ];
  for (const [sale, renew, unit] of cycles) {
    if (sale == null) continue;
    return { price: renewing && renew != null ? renew : sale, unit };
  }
  return { price: plan.price, unit: '月' };
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

const CardsOffersPage: React.FC = () => {
  const { campaign, missing, leave, buy, symbol, copyright } = useStorefrontPage('cards');
  const plans = campaign?.plans || [];

  return (
    <div className="order-standalone coupon-storefront is-dark">
      <Button className="order-standalone-back" type="text" icon={<ArrowLeftOutlined />} onClick={leave}>
        返回
      </Button>
      {missing && <div className="credits-note">活动不存在或未启用。</div>}
      {campaign && (
        <div>
          <h2 className="pg-ig-hplan-title">{campaign.headline || '热门活动'}</h2>
          {campaign.subtitle ? <p className="pg-ig-campaign-sub">{campaign.subtitle}</p> : null}
          {campaign.description ? (
            <p className="pg-ig-campaign-sub" style={{ whiteSpace: 'pre-line', opacity: 0.8, marginTop: 4 }}>
              {campaign.description}
            </p>
          ) : null}
          <div className="pg-ig-campaign-grid">
            {plans.map((plan) => {
              const display = shownPrice(plan);
              return (
                <article key={plan.id} className="pg-ig-campaign-card">
                  <div className="pg-ig-campaign-name">{plan.name}</div>
                  <div className="pg-ig-campaign-desc">{plan.subtitle || plan.description}</div>
                  <div className="pg-ig-campaign-price">
                    {formatCreditMoney(symbol, display.price)}
                    <span>{` / ${display.unit}`}</span>
                  </div>
                  {(plan.features?.length || (plan.credit_enabled === 1 && plan.model_ids?.length)) ? (
                    <ul className="pg-ig-campaign-features">
                      {(plan.features || []).map((line) => <li key={line}>{line}</li>)}
                      {plan.credit_enabled === 1 && plan.model_ids?.length ? (
                        <li>适用模型：{plan.model_ids.join('、')}</li>
                      ) : null}
                    </ul>
                  ) : null}
                  <Button type="primary" block disabled={plan.action !== 'buy'} onClick={() => buy(plan)}>{buyLabel(plan)}</Button>
                </article>
              );
            })}
          </div>
          {!plans.length && <div className="credits-note">这里还没有可展示的卡券方案。</div>}
        </div>
      )}
      {copyright ? <div className="coupon-storefront-copyright">{copyright}</div> : null}
    </div>
  );
};

export default CardsOffersPage;
