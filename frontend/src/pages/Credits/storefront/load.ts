/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import { useEffect, useRef, useState } from 'react';
import { useLocation, useNavigate, useParams } from 'react-router-dom';
import request from '../../../utils/request';
import useSettingsStore from '../../../store/settings';
import {
  clearOfferReturn,
  couponCampaignPath,
  offerReturnFromState,
  offerReturnState,
  safeOfferReturnPath,
} from './path';

export type StorefrontPlan = {
  id: number;
  name: string;
  subtitle: string;
  description: string;
  price: number;
  accent?: string;
  badge?: string;
  features?: string[];
  action: string;
  tier_enabled?: number;
  target_group_key?: string;
  tier_duration_mode?: string;
  tier_duration_days?: number;
  tier_duration_hours?: number;
  credit_enabled?: number;
  credit_amount?: number;
  credit_worth?: number;
  credit_discount?: number;
  credit_model_discounts?: Record<string, number>;
  credit_duration_mode?: string;
  credit_duration_days?: number;
  credit_duration_hours?: number;
  model_ids?: string[];
  space_enabled?: number;
  space_mb?: number;
  space_duration_mode?: string;
  space_duration_days?: number;
  space_duration_hours?: number;
  gift_space_mb?: number;
  gift_space_duration_mode?: string;
  gift_space_duration_days?: number;
  gift_space_duration_hours?: number;
  limit_mode?: string;
  limit_count?: number;
  limit_window_days?: number;
  limit_window_hours?: number;
  purchased_count?: number;
  sale_limit_total?: number;
  sale_limit_daily?: number;
  sale_limit_window_days?: number;
  sale_limit_window_count?: number;
  sold_total_count?: number;
  sold_daily_count?: number;
  sold_window_count?: number;
  sale_limit_reason?: string | null;
  offer_kind?: string;
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

export function planSaleLimitSummary(plan: StorefrontPlan): string | null {
  const parts: string[] = [];
  if ((plan.sale_limit_daily || 0) > 0) {
    parts.push(`今日限量 ${plan.sold_daily_count ?? 0}/${plan.sale_limit_daily} 份`);
  }
  if ((plan.sale_limit_window_days || 0) > 0 && (plan.sale_limit_window_count || 0) > 0) {
    parts.push(`${plan.sale_limit_window_days}天限量 ${plan.sold_window_count ?? 0}/${plan.sale_limit_window_count} 份`);
  }
  if ((plan.sale_limit_total || 0) > 0) {
    parts.push(`总限量 ${plan.sold_total_count ?? 0}/${plan.sale_limit_total} 份`);
  }
  return parts.length > 0 ? parts.join(' · ') : null;
}

type StorefrontCampaign = {
  id: number;
  headline: string;
  subtitle: string;
  description?: string;
  template_key: string;
  plans: StorefrontPlan[];
  level_name?: string;
  balance?: number;
};

export function useStorefrontPage(expectedTemplate: string) {
  const { campaignId } = useParams<{ campaignId: string }>();
  const navigate = useNavigate();
  const location = useLocation();
  const returnPath = useRef('');
  returnPath.current = offerReturnFromState(location.state)
    || safeOfferReturnPath(new URLSearchParams(location.search).get('from'));
  const { settings, fetchSettings } = useSettingsStore();
  const symbol = settings?.currency?.currency_symbol || '¥';
  const unit = settings?.currency?.currency_unit || '';
  const copyright = settings?.site?.copyright?.trim() || '';
  const [campaign, setCampaign] = useState<StorefrontCampaign | null>(null);
  const [missing, setMissing] = useState(false);

  useEffect(() => {
    if (!settings) {
      void fetchSettings();
    }
  }, [settings, fetchSettings]);

  useEffect(() => {
    let dead = false;
    setMissing(false);
    setCampaign(null);
    Promise.all([
      request.get(`/plugins/marketing_membership/storefront/${campaignId}`) as Promise<StorefrontCampaign>,
      (request.get('/models', { skipErrorHandler: true } as any) as Promise<{
        data?: Array<{ mid?: string; model_id?: string; name?: string }>;
      }>).catch(() => ({ data: [] })),
    ])
      .then(([res, modelsRes]) => {
        if (dead) return;
        const template = res.template_key || 'cards';
        if (template !== expectedTemplate) {
          const from = returnPath.current;
          navigate(couponCampaignPath(template, res.id || campaignId || ''), {
            replace: true,
            state: from ? { from } : undefined,
          });
          return;
        }
        const nameMap: Record<string, string> = {};
        (modelsRes?.data || []).forEach((item) => {
          const displayName = (item.name || '').trim() || (item.model_id || '').trim();
          if (!displayName) return;
          const mid = (item.mid || '').trim();
          const modelId = (item.model_id || '').trim();
          if (mid && !nameMap[mid]) nameMap[mid] = displayName;
          if (modelId && !nameMap[modelId]) nameMap[modelId] = displayName;
        });
        const resolvedPlans = (res.plans || []).map((plan) => {
          const rawModels = plan.model_ids || [];
          const modelIds = Array.from(
            new Set(
              rawModels
                .map((m) => m.trim())
                .filter(Boolean)
                .map((m) => nameMap[m] || m),
            ),
          );
          const rawDiscounts = plan.credit_model_discounts || {};
          const resolvedDiscounts: Record<string, number> = {};
          Object.entries(rawDiscounts).forEach(([k, v]) => {
            const key = nameMap[k.trim()] || k.trim();
            if (!key) return;
            if (resolvedDiscounts[key] === undefined || v < resolvedDiscounts[key]) {
              resolvedDiscounts[key] = v;
            }
          });
          return {
            ...plan,
            model_ids: modelIds,
            credit_model_discounts: resolvedDiscounts,
          };
        });
        let desc = res.description || '';
        if (!desc) {
          try {
            desc = localStorage.getItem(`mkt_campaign_desc_${res.id || campaignId}`) || '';
          } catch {
            /* ignore */
          }
        }
        setCampaign({ ...res, description: desc, plans: resolvedPlans });
      })
      .catch(() => {
        if (!dead) setMissing(true);
      });
    return () => {
      dead = true;
    };
  }, [campaignId, expectedTemplate, navigate]);

  useEffect(() => {
    const params = new URLSearchParams(location.search);
    const queryFrom = safeOfferReturnPath(params.get('from'));
    if (!queryFrom) return;
    params.delete('from');
    const search = params.toString();
    offerReturnState(queryFrom);
    navigate(
      { pathname: location.pathname, search: search ? `?${search}` : '' },
      { replace: true, state: { from: queryFrom } },
    );
  }, [location.pathname, location.search, navigate]);

  const leave = () => {
    const target = returnPath.current;
    if (target) {
      clearOfferReturn();
      navigate(target, { replace: true });
      return;
    }
    if (location.key !== 'default') {
      navigate(-1);
      return;
    }
    navigate('/dashboard', { replace: true });
  };

  const buy = (plan: StorefrontPlan) => {
    if (plan.action !== 'buy') return;
    navigate(`/orders/checkout/${plan.id}`);
  };

  return { campaign, missing, leave, buy, symbol, unit, copyright };
}
