/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

const SEGMENTS: Record<string, string> = {
  cards: 'cards',
  h_site: 'h-site',
  i_site: 'i-site',
  tke: 'tke',
  tke_coupon: 'tke-coupon',
};

export function couponCampaignPath(templateKey: string | undefined, campaignId: number | string) {
  const segment = SEGMENTS[templateKey || ''] || 'cards';
  return `/offers/${segment}/${campaignId}`;
}

export function couponCampaignRouteLabel(templateKey: string | undefined) {
  const segment = SEGMENTS[templateKey || ''] || 'cards';
  return `/offers/${segment}/:活动ID`;
}

const OFFER_RETURN_KEY = 'mkt_offer_return_path';

export function safeOfferReturnPath(raw: string | null | undefined) {
  if (!raw || !raw.startsWith('/') || raw.startsWith('//') || raw.includes('\\') || raw.includes('://')) return '';
  if (raw.startsWith('/offers') || raw.startsWith('/login') || raw.startsWith('/register') || raw.startsWith('/orders')) return '';
  return raw;
}

export function offerReturnState(fromPath: string) {
  const from = safeOfferReturnPath(fromPath);
  if (!from) return undefined;
  try {
    sessionStorage.setItem(OFFER_RETURN_KEY, from);
  } catch {
    /* 隐私模式写不进会话存储时，仍靠路由 state 返回 */
  }
  return { from };
}

export function offerReturnFromState(state: unknown) {
  if (!state || typeof state !== 'object') return '';
  return safeOfferReturnPath((state as { from?: string }).from);
}

export function rememberedOfferReturn() {
  try {
    return safeOfferReturnPath(sessionStorage.getItem(OFFER_RETURN_KEY));
  } catch {
    return '';
  }
}

export function clearOfferReturn() {
  try {
    sessionStorage.removeItem(OFFER_RETURN_KEY);
  } catch {
    /* 清不掉也不影响这次返回 */
  }
}
